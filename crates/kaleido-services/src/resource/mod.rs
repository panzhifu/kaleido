//! The **resource** manager implementation — document resources
//! (fonts / swatches / brushes).
//!
//! Backed by an [`RwLock`] over a [`HashMap`] plus a monotonic id counter,
//! so every operation is safe to call from any number of threads.
//!
//! # Lock poisoning policy
//!
//! A panic while a lock is held poisons it, but the guarded data stays
//! valid. All methods recover the guard via [`recover`] instead of failing
//! or degrading to empty results, so one panicked caller cannot wedge the
//! service for everyone else.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_core::ResourceId;
use kaleido_traits::services::resource::{ResourceData, ResourceKind, ResourceService};
use kaleido_traits::services::{ServiceError, ServiceResult};

/// Recovers the guarded value from a poisoned lock.
///
/// The data behind a poisoned [`RwLock`] is still valid — only the flag
/// recording the panic is set — so taking the inner value back keeps the
/// service operational.
fn recover<T>(poisoned: std::sync::PoisonError<T>) -> T {
    poisoned.into_inner()
}

/// Default implementation of [`ResourceService`].
pub struct ResourceServiceImpl {
    // Kept for future event emission, matching the other manager services.
    ctx: Context,
    store: RwLock<HashMap<ResourceId, ResourceData>>,
    next_id: AtomicU64,
}

impl ResourceServiceImpl {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            store: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

impl Service for ResourceServiceImpl {
    const NAME: &'static str = "resource_service";
}

impl ResourceService for ResourceServiceImpl {
    fn register(&self, data: ResourceData) -> ServiceResult<ResourceId> {
        // Monotonic allocation starting at 1. Ids are never reused, so
        // register always inserts a fresh entry and can never overwrite an
        // existing one — call `update` for in-place replacement. Relaxed
        // ordering suffices: uniqueness comes from `fetch_add`'s atomicity,
        // not from cross-thread memory ordering.
        let id = ResourceId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.store.write().unwrap_or_else(recover).insert(id, data);
        Ok(id)
    }

    fn get(&self, id: ResourceId) -> Option<ResourceData> {
        self.store.read().unwrap_or_else(recover).get(&id).cloned()
    }

    fn update(&self, id: ResourceId, data: ResourceData) -> ServiceResult<()> {
        let mut store = self.store.write().unwrap_or_else(recover);
        if !store.contains_key(&id) {
            return Err(ServiceError::ResourceNotFound(id.0));
        }
        store.insert(id, data);
        Ok(())
    }

    fn remove(&self, id: ResourceId) -> ServiceResult<()> {
        let mut store = self.store.write().unwrap_or_else(recover);
        if store.remove(&id).is_none() {
            return Err(ServiceError::ResourceNotFound(id.0));
        }
        Ok(())
    }

    fn list(&self, kind: ResourceKind) -> Vec<(ResourceId, ResourceData)> {
        let store = self.store.read().unwrap_or_else(recover);
        let mut matched: Vec<(ResourceId, ResourceData)> = store
            .iter()
            .filter(|(_, data)| data.kind() == kind)
            .map(|(id, data)| (*id, data.clone()))
            .collect();
        // Deterministic output regardless of HashMap iteration order.
        matched.sort_by_key(|(id, _)| *id);
        matched
    }

    fn count(&self) -> usize {
        self.store.read().unwrap_or_else(recover).len()
    }
}

/// Installs the `resource_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<ResourceServiceImpl, (), _>(
        "resource_service",
        Inject::none(),
        |ctx, _config| Ok(ResourceServiceImpl::new(ctx)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::Color;
    use std::sync::Arc;

    fn service() -> ResourceServiceImpl {
        ResourceServiceImpl::new(Context::new())
    }

    fn font() -> ResourceData {
        ResourceData::Font {
            name: "Inter".into(),
            bytes: vec![1, 2, 3, 4],
        }
    }

    fn swatch() -> ResourceData {
        ResourceData::Swatch {
            color: Color::white(),
        }
    }

    fn brush(name: &str) -> ResourceData {
        ResourceData::Brush { name: name.into() }
    }

    #[test]
    fn register_get_remove_list_count_flow() {
        let svc = service();
        assert_eq!(svc.count(), 0);
        assert!(svc.get(ResourceId(1)).is_none());

        // Ids are allocated monotonically from 1.
        let font_id = svc.register(font()).unwrap();
        let swatch_id = svc.register(swatch()).unwrap();
        let brush_id = svc.register(brush("Round")).unwrap();
        assert_eq!(
            (font_id, swatch_id, brush_id),
            (ResourceId(1), ResourceId(2), ResourceId(3))
        );
        assert_eq!(svc.count(), 3);

        // get round-trips the payload.
        assert_eq!(svc.get(font_id), Some(font()));
        assert!(svc.get(ResourceId(999)).is_none());

        // list filters by kind.
        assert_eq!(svc.list(ResourceKind::Font), vec![(font_id, font())]);
        assert_eq!(
            svc.list(ResourceKind::Swatch),
            vec![(swatch_id, swatch())]
        );
        assert_eq!(
            svc.list(ResourceKind::Brush),
            vec![(brush_id, brush("Round"))]
        );

        // remove deletes and reports missing ids.
        svc.remove(swatch_id).unwrap();
        assert_eq!(svc.count(), 2);
        assert!(svc.get(swatch_id).is_none());
        assert!(matches!(
            svc.remove(swatch_id),
            Err(ServiceError::ResourceNotFound(2))
        ));
        assert!(svc.list(ResourceKind::Swatch).is_empty());
    }

    #[test]
    fn register_never_reuses_ids() {
        let svc = service();
        let a = svc.register(font()).unwrap();
        let b = svc.register(font()).unwrap();
        assert_ne!(a, b);
        assert_eq!(svc.count(), 2);
        // Both entries coexist under their own ids.
        assert_eq!(svc.get(a), Some(font()));
        assert_eq!(svc.get(b), Some(font()));
    }

    #[test]
    fn update_overwrites_payload_in_place() {
        let svc = service();
        let id = svc.register(font()).unwrap();
        let replacement = ResourceData::Font {
            name: "Inter Bold".into(),
            bytes: vec![9, 9, 9],
        };

        svc.update(id, replacement.clone()).unwrap();
        // The id and the total count are unchanged; the payload is replaced.
        assert_eq!(svc.get(id), Some(replacement.clone()));
        assert_eq!(svc.count(), 1);
        // Kind filtering follows the current payload.
        assert_eq!(svc.list(ResourceKind::Font), vec![(id, replacement)]);
    }

    #[test]
    fn update_missing_id_reports_not_found() {
        let svc = service();
        assert!(matches!(
            svc.update(ResourceId(42), font()),
            Err(ServiceError::ResourceNotFound(42))
        ));
    }

    #[test]
    fn remove_missing_id_reports_not_found() {
        let svc = service();
        assert!(matches!(
            svc.remove(ResourceId(7)),
            Err(ServiceError::ResourceNotFound(7))
        ));
    }

    #[test]
    fn list_filters_by_kind_in_id_order() {
        let svc = service();
        // Register in mixed order so kind filters and id ordering are both
        // exercised independently of registration order.
        let b1 = svc.register(brush("A")).unwrap(); // id 1
        let s1 = svc.register(swatch()).unwrap(); // id 2
        let f1 = svc.register(font()).unwrap(); // id 3
        let s2 = svc.register(swatch()).unwrap(); // id 4
        let f2 = svc.register(font()).unwrap(); // id 5

        assert_eq!(svc.list(ResourceKind::Brush), vec![(b1, brush("A"))]);
        assert_eq!(
            svc.list(ResourceKind::Swatch),
            vec![(s1, swatch()), (s2, swatch())]
        );
        assert_eq!(
            svc.list(ResourceKind::Font),
            vec![(f1, font()), (f2, font())]
        );

        // Removing an entry drops it from its kind's list.
        svc.remove(s1).unwrap();
        assert_eq!(svc.list(ResourceKind::Swatch), vec![(s2, swatch())]);
    }

    #[test]
    fn concurrent_registers_allocate_unique_ids() {
        let svc = Arc::new(service());
        const THREADS: usize = 8;
        const PER_THREAD: usize = 64;

        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                let svc = Arc::clone(&svc);
                scope.spawn(move || {
                    for _ in 0..PER_THREAD {
                        svc.register(font()).unwrap();
                    }
                });
            }
        });

        assert_eq!(svc.count(), THREADS * PER_THREAD);
        let mut ids: Vec<ResourceId> = svc
            .list(ResourceKind::Font)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            THREADS * PER_THREAD,
            "every concurrent register must get a unique id"
        );
    }

    #[test]
    fn concurrent_register_remove_get_stay_consistent() {
        let svc = Arc::new(service());
        // Pre-register a pool; threads concurrently remove every pool entry
        // (races are fine — a second removal reports NotFound), register new
        // fonts, and read the store.
        let pool: Vec<ResourceId> = (0..16)
            .map(|_| svc.register(brush("pool")).unwrap())
            .collect();

        std::thread::scope(|scope| {
            for _ in 0..4 {
                let svc = Arc::clone(&svc);
                let pool = pool.clone();
                scope.spawn(move || {
                    for id in pool {
                        let _ = svc.remove(id);
                    }
                    for _ in 0..8 {
                        svc.register(font()).unwrap();
                    }
                    let _ = svc.count();
                    let _ = svc.get(ResourceId(1));
                });
            }
        });

        // No panic, no duplicate ids: whatever brushes remain are a subset
        // of the pool, and every registered font id is unique.
        let remaining: Vec<ResourceId> = svc
            .list(ResourceKind::Brush)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        for id in &remaining {
            assert!(pool.contains(id), "unexpected brush id {id:?}");
        }
        let mut font_ids: Vec<ResourceId> = svc
            .list(ResourceKind::Font)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        font_ids.sort();
        font_ids.dedup();
        assert_eq!(font_ids.len(), svc.list(ResourceKind::Font).len());
    }

    #[test]
    fn plugin_installs_service() {
        let ctx = Context::new();
        ctx.plugin(plugin(), ());
        let svc: Arc<dyn ResourceService> = ctx.require::<ResourceServiceImpl>("resource_service").unwrap();
        let id = svc.register(font()).unwrap();
        assert_eq!(svc.count(), 1);
        assert!(svc.get(id).is_some());
    }
}

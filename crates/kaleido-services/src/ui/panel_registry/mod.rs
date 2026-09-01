//! Panel registry — manages plugin-supplied panels.
//!
//! The [`PanelRegistry`] contract lives in `kaleido-traits`; this module
//! provides the default implementation plus the Cordis plugin and resolver.
//!
//! Panels are held **weakly**: the plugin keeps the strong `Arc` alive, and
//! dead weak references are swept on every read so the registry never leaks
//! entries and never reports a panel whose owner is gone.

use std::sync::{Arc, Mutex, MutexGuard, Weak};

use cordis::{Inject, PluginHandle};
use kaleido_traits::{Panel, PanelRegistry};

/// Resolves the panel registry from a Cordis context.
pub fn resolve_panel_registry(
    ctx: &cordis::Context,
) -> cordis::Result<Arc<dyn PanelRegistry>> {
    let inner = ctx
        .get::<Arc<dyn PanelRegistry>>("panel_registry")?
        .ok_or_else(|| {
            cordis::CordisError::with_message(
                cordis::ErrorCode::MissingService,
                "panel_registry service is not available",
            )
        })?;
    Ok(inner.as_ref().clone())
}

// ---------------------------------------------------------------------------
// PanelEntry — internal bookkeeping
// ---------------------------------------------------------------------------

/// One registered panel, held by weak reference.
///
/// Wrapped in a struct so future bookkeeping (ids, ordering, the owning
/// plugin) can be added without changing the [`PanelRegistry`] contract.
#[derive(Debug)]
pub struct PanelEntry {
    /// Weak handle to the panel; dead once the plugin drops its strong `Arc`.
    pub panel: Weak<Mutex<dyn Panel>>,
}

// ---------------------------------------------------------------------------
// PanelRegistryImpl
// ---------------------------------------------------------------------------

/// Default implementation of [`PanelRegistry`].
///
/// The internal list is lock-protected and safe to touch from any thread.
/// The lock is *recovered* on poisoning: the guarded `Vec` is not corrupted
/// by a panic in a concurrent caller, so the poison error is unwrapped and
/// the operation proceeds.
#[derive(Debug, Default)]
pub struct PanelRegistryImpl {
    panels: std::sync::Mutex<Vec<PanelEntry>>,
}

impl PanelRegistryImpl {
    /// Creates an empty panel registry.
    pub fn new() -> Self {
        Self {
            panels: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Acquires the panel list, recovering from poisoning.
    fn lock_panels(&self) -> MutexGuard<'_, Vec<PanelEntry>> {
        self.panels
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Drops dead weak pointers. Caller must hold the lock.
    fn sweep_locked(panels: &mut Vec<PanelEntry>) {
        panels.retain(|entry| entry.panel.upgrade().is_some());
    }
}

impl PanelRegistry for PanelRegistryImpl {
    fn register(&self, panel: Weak<Mutex<dyn Panel>>) {
        // Appends unconditionally: re-registering the same panel creates a
        // second entry. The registry never owns the panel (weak only), so
        // this cannot keep a dead panel alive.
        self.lock_panels().push(PanelEntry { panel });
    }

    /// Removes the panel at `index`, if present.
    ///
    /// Note: indices are stable only between calls that sweep — a
    /// [`PanelRegistry::panels`] / [`PanelRegistry::len`] call removes dead
    /// entries and therefore shifts subsequent indices. Callers that track
    /// indices should treat them as ephemeral.
    fn unregister(&self, index: usize) {
        let mut panels = self.lock_panels();
        if index < panels.len() {
            panels.remove(index);
        }
    }

    fn panels(&self) -> Vec<Arc<Mutex<dyn Panel>>> {
        let mut panels = self.lock_panels();
        Self::sweep_locked(&mut panels);
        // Sweep guarantees every surviving weak ref upgrades.
        panels.iter().filter_map(|entry| entry.panel.upgrade()).collect()
    }

    fn len(&self) -> usize {
        let mut panels = self.lock_panels();
        Self::sweep_locked(&mut panels);
        panels.len()
    }
}

// ---------------------------------------------------------------------------
// Cordis plugin
// ---------------------------------------------------------------------------

/// Plugin that installs the [`PanelRegistry`] service.
///
/// Plugins provide panels via `register`; the host reads live panels each
/// frame and renders them in its side panel area.
pub fn panel_registry_plugin() -> PluginHandle {
    cordis::plugin_sync::<(), _>("panel_registry", Inject::none(), |ctx, _config| {
        let registry: Arc<dyn PanelRegistry> = Arc::new(PanelRegistryImpl::new());
        ctx.provide("panel_registry", registry)?;
        Ok(cordis::PluginOutput::none())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_traits::PanelContext;

    /// A minimal panel for registry tests.
    struct TestPanel;

    impl Panel for TestPanel {
        fn render(&mut self, _ctx: &mut dyn PanelContext) {}
    }

    fn registry() -> PanelRegistryImpl {
        PanelRegistryImpl::new()
    }

    fn test_panel() -> Arc<Mutex<dyn Panel>> {
        Arc::new(Mutex::new(TestPanel))
    }

    #[test]
    fn empty_registry_reports_no_panels() {
        let registry = registry();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
        assert!(registry.panels().is_empty());
    }

    #[test]
    fn register_and_list_panels() {
        let registry = registry();
        let a = test_panel();
        let b = test_panel();
        registry.register(Arc::downgrade(&a));
        registry.register(Arc::downgrade(&b));

        let panels = registry.panels();
        assert_eq!(panels.len(), 2);
        assert!(Arc::ptr_eq(&panels[0], &a));
        assert!(Arc::ptr_eq(&panels[1], &b));
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
    }

    #[test]
    fn dead_panels_are_swept() {
        let registry = registry();
        {
            let panel = test_panel();
            registry.register(Arc::downgrade(&panel));
            assert_eq!(registry.len(), 1);
            // panel dropped here → weak ref goes dead.
        }
        // Both listing and len sweep the dead entry.
        assert!(registry.panels().is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
        // The dead entry was physically removed, not merely hidden.
        assert_eq!(registry.panels.lock().unwrap().len(), 0);
    }

    #[test]
    fn unregister_removes_by_index() {
        let registry = registry();
        let a = test_panel();
        let b = test_panel();
        registry.register(Arc::downgrade(&a));
        registry.register(Arc::downgrade(&b));

        registry.unregister(0);
        let panels = registry.panels();
        assert_eq!(panels.len(), 1);
        assert!(Arc::ptr_eq(&panels[0], &b));

        // Out-of-bounds unregister is a no-op.
        registry.unregister(99);
        assert_eq!(registry.panels().len(), 1);
    }

    #[test]
    fn dead_registration_is_ignored_by_sweep() {
        let registry = registry();
        // Register a weak ref that is already dead.
        {
            let panel = test_panel();
            let weak = Arc::downgrade(&panel);
            registry.register(weak);
        }
        assert!(registry.panels().is_empty());
    }

    #[test]
    fn registry_survives_concurrent_registration() {
        let registry = Arc::new(registry());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let registry = registry.clone();
            // Each thread keeps its panels alive and returns the strong refs.
            handles.push(std::thread::spawn(move || {
                let mut alive = Vec::new();
                for _ in 0..50 {
                    let panel = test_panel();
                    registry.register(Arc::downgrade(&panel));
                    alive.push(panel);
                }
                alive
            }));
        }
        let mut keepalive = Vec::new();
        for handle in handles {
            keepalive.extend(handle.join().unwrap());
        }
        // All 400 weak refs are live while the strong refs exist.
        assert_eq!(registry.len(), 400);
        assert_eq!(registry.panels().len(), 400);

        // Dropping the strong refs makes every entry dead and swept.
        drop(keepalive);
        assert_eq!(registry.len(), 0);
        assert!(registry.panels().is_empty());
    }
}

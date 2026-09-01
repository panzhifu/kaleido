//! The **history manager** implementation.
//!
//! Manages undo/redo stacks with COW (copy-on-write) snapshots.
//! Unmodified tiles are shared via Arc, so cloning a Document is cheap.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_core::Document;
use kaleido_traits::data::error::{ServiceError, ServiceResult};
use kaleido_traits::data::DataService;
use kaleido_traits::history::{HistoryEntry, HistoryService, Snapshot};

/// Maximum number of undo snapshots retained.
const UNDO_LIMIT: usize = 100;

/// Current unix timestamp in seconds.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── HistoryServiceImpl ───────────────────────────────────────────────────

/// Default implementation of [`HistoryService`].
pub struct HistoryServiceImpl {
    ctx: Context,
    data_service: Arc<dyn DataService>,
    /// Undo stack — COW snapshots of state *before* each mutation.
    undo: RwLock<Vec<(Document, HistoryEntry)>>,
    /// Redo stack — states moved out of the way by undo.
    redo: RwLock<Vec<(Document, HistoryEntry)>>,
    /// Entry id counter.
    next_id: AtomicU64,
}

impl HistoryServiceImpl {
    pub fn new(ctx: Context, data_service: Arc<dyn DataService>) -> Self {
        Self {
            ctx,
            data_service,
            undo: RwLock::new(Vec::new()),
            redo: RwLock::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Pushes a new undo entry.
    pub fn push(&self, before_snapshot: Document, label: &str) -> ServiceResult<()> {
        let entry = HistoryEntry {
            id: self.next_id.fetch_add(1, Ordering::SeqCst),
            label: label.into(),
            timestamp: now_secs(),
        };

        let mut undo = self.undo.write().unwrap_or_else(|e| e.into_inner());
        if undo.len() >= UNDO_LIMIT {
            undo.remove(0);
        }
        undo.push((before_snapshot, entry));

        let mut redo = self.redo.write().unwrap_or_else(|e| e.into_inner());
        redo.clear();

        Ok(())
    }

    /// Snapshots the current document state onto the undo stack.
    pub fn snapshot(&self) -> ServiceResult<()> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        self.push(doc, "mutation")
    }
}

impl Service for HistoryServiceImpl {
    const NAME: &'static str = "history_service";
}

/// Installs the `history_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<HistoryServiceImpl, (), _>(
        "history_service",
        Inject::none(),
        |ctx, _config| {
            let data_service: Arc<dyn DataService> = ctx
                .get::<crate::data::DataServiceImpl>("data_service")?
                .ok_or_else(|| -> cordis::CordisError {
                    cordis::CordisError::with_message(
                        cordis::ErrorCode::Other,
                        String::from("data_service not found"),
                    )
                })?;
            Ok(HistoryServiceImpl::new(ctx, data_service))
        },
    )
}

// ── HistoryService trait implementation ───────────────────────────────────

impl HistoryService for HistoryServiceImpl {
    fn undo(&self) -> ServiceResult<()> {
        let (snapshot, _entry) = {
            let mut undo = self.undo.write().unwrap_or_else(|e| e.into_inner());
            undo.pop()
                .ok_or_else(|| ServiceError::Other("nothing to undo".into()))?
        };

        // Push current state to redo stack.
        let current = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let redo_entry = HistoryEntry {
            id: self.next_id.fetch_add(1, Ordering::SeqCst),
            label: "undo".into(),
            timestamp: now_secs(),
        };
        {
            let mut redo = self.redo.write().unwrap_or_else(|e| e.into_inner());
            redo.push((current, redo_entry));
        }

        // Restore the snapshot via DataService.
        self.data_service.restore(snapshot);
        Ok(())
    }

    fn redo(&self) -> ServiceResult<()> {
        let (snapshot, _entry) = {
            let mut redo = self.redo.write().unwrap_or_else(|e| e.into_inner());
            redo.pop()
                .ok_or_else(|| ServiceError::Other("nothing to redo".into()))?
        };

        // Push current state to undo stack.
        let current = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let undo_entry = HistoryEntry {
            id: self.next_id.fetch_add(1, Ordering::SeqCst),
            label: "redo".into(),
            timestamp: now_secs(),
        };
        {
            let mut undo = self.undo.write().unwrap_or_else(|e| e.into_inner());
            undo.push((current, undo_entry));
        }

        // Restore the snapshot via DataService.
        self.data_service.restore(snapshot);
        Ok(())
    }

    fn can_undo(&self) -> bool {
        let undo = self.undo.read().unwrap_or_else(|e| e.into_inner());
        !undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        let redo = self.redo.read().unwrap_or_else(|e| e.into_inner());
        !redo.is_empty()
    }

    fn undo_depth(&self) -> usize {
        let undo = self.undo.read().unwrap_or_else(|e| e.into_inner());
        undo.len()
    }

    fn redo_depth(&self) -> usize {
        let redo = self.redo.read().unwrap_or_else(|e| e.into_inner());
        redo.len()
    }

    fn last_label(&self) -> Option<String> {
        let undo = self.undo.read().unwrap_or_else(|e| e.into_inner());
        undo.last().map(|(_, entry)| entry.label.clone())
    }

    fn clear(&self) -> ServiceResult<()> {
        let mut undo = self.undo.write().unwrap_or_else(|e| e.into_inner());
        undo.clear();
        let mut redo = self.redo.write().unwrap_or_else(|e| e.into_inner());
        redo.clear();
        Ok(())
    }

    fn undo_entries(&self) -> Vec<HistoryEntry> {
        let undo = self.undo.read().unwrap_or_else(|e| e.into_inner());
        undo.iter().map(|(_, entry)| entry.clone()).rev().collect()
    }

    fn redo_entries(&self) -> Vec<HistoryEntry> {
        let redo = self.redo.read().unwrap_or_else(|e| e.into_inner());
        redo.iter().map(|(_, entry)| entry.clone()).rev().collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{DocumentId, ImageSize};

    fn make_doc(name: &str) -> Document {
        Document::new(DocumentId(1), name, 100, 100).unwrap()
    }

    /// A minimal fake DataService for unit testing HistoryService.
    struct FakeDataService {
        doc: RwLock<Option<Document>>,
    }

    impl FakeDataService {
        fn new(doc: Document) -> Self {
            Self {
                doc: RwLock::new(Some(doc)),
            }
        }
    }

    impl DataService for FakeDataService {
        fn new_document(&self, _name: &str, _w: u32, _h: u32) -> ServiceResult<kaleido_core::DocumentId> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn open(&self, _path: &std::path::Path) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn save(&self) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn save_as(&self, _path: &std::path::Path) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn close(&self) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn document(&self) -> ServiceResult<Option<Document>> {
            Ok(self.doc.read().unwrap_or_else(|e| e.into_inner()).clone())
        }
        fn has_document(&self) -> bool {
            self.doc.read().unwrap_or_else(|e| e.into_inner()).is_some()
        }
        fn path(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn size(&self) -> Option<ImageSize> {
            None
        }
        fn restore(&self, snapshot: Document) {
            *self.doc.write().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
        }
        fn restore_snapshot(&self, snapshot: &kaleido_traits::history::Snapshot) {
            match snapshot {
                kaleido_traits::history::Snapshot::Full(doc) => {
                    *self.doc.write().unwrap_or_else(|e| e.into_inner()) = Some(doc.clone());
                }
                kaleido_traits::history::Snapshot::DirtyTile(dirty) => {
                    let mut doc_guard = self.doc.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(ref mut doc) = *doc_guard {
                        doc.name = dirty.name.clone();
                    }
                }
            }
        }
    }

    fn make_service() -> (HistoryServiceImpl, Arc<FakeDataService>) {
        let doc = make_doc("test");
        let fake = Arc::new(FakeDataService::new(doc));
        let svc = HistoryServiceImpl::new(Context::new(), fake.clone());
        (svc, fake)
    }

    #[test]
    fn test_new_service_is_empty() {
        let (svc, _fake) = make_service();
        assert!(!svc.can_undo());
        assert!(!svc.can_redo());
        assert_eq!(svc.undo_depth(), 0);
        assert_eq!(svc.redo_depth(), 0);
        assert!(svc.last_label().is_none());
    }

    #[test]
    fn test_snapshot_and_undo_redo() {
        let (svc, fake) = make_service();

        // Snapshot the current state.
        svc.snapshot().unwrap();
        assert!(svc.can_undo());
        assert_eq!(svc.undo_depth(), 1);

        // Restore a different document to simulate mutation.
        let modified = make_doc("modified");
        fake.restore(modified);
        assert_eq!(fake.document().unwrap().unwrap().name, "modified");

        // Undo restores the snapshot.
        svc.undo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "test");
        assert!(svc.can_redo());

        // Redo re-applies.
        svc.redo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "modified");
        assert!(svc.can_undo());
    }

    #[test]
    fn test_clear() {
        let (svc, _fake) = make_service();
        svc.snapshot().unwrap();
        assert!(svc.can_undo());

        svc.clear().unwrap();
        assert!(!svc.can_undo());
        assert_eq!(svc.undo_depth(), 0);
    }

    #[test]
    fn test_undo_redo_entries() {
        let (svc, _fake) = make_service();
        svc.push(make_doc("doc1"), "op1").unwrap();
        svc.push(make_doc("doc2"), "op2").unwrap();

        let entries = svc.undo_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "op2"); // Most recent first
        assert_eq!(entries[1].label, "op1");
    }

    #[test]
    fn test_undo_limit() {
        let (svc, _fake) = make_service();

        for i in 0..UNDO_LIMIT + 20 {
            svc.push(make_doc(&format!("doc{i}")), &format!("op{i}"))
                .unwrap();
        }

        assert_eq!(svc.undo_depth(), UNDO_LIMIT);
    }

    #[test]
    fn test_multiple_undo_redo_cycles() {
        let (svc, fake) = make_service();

        // Push 3 snapshots.
        svc.snapshot().unwrap();
        fake.restore(make_doc("state1"));
        svc.snapshot().unwrap();
        fake.restore(make_doc("state2"));
        svc.snapshot().unwrap();
        fake.restore(make_doc("state3"));

        assert_eq!(svc.undo_depth(), 3);

        // Undo all.
        svc.undo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "state2");
        svc.undo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "state1");
        svc.undo().unwrap();

        // Redo all.
        svc.redo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "state1");
        svc.redo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "state2");
        svc.redo().unwrap();
        assert_eq!(fake.document().unwrap().unwrap().name, "state3");
    }

    #[test]
    fn test_new_mutation_clears_redo() {
        let (svc, fake) = make_service();

        svc.snapshot().unwrap();
        fake.restore(make_doc("modified"));
        svc.undo().unwrap();

        // Now redo should be available.
        assert!(svc.can_redo());

        // A new snapshot should clear the redo stack.
        svc.snapshot().unwrap();
        assert!(!svc.can_redo());
    }
}

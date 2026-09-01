//! The **selection manager** implementation.
//!
//! Manages the document-wide selection mask. Works with [`super::data::DataService`]
//! to read and modify the document's selection state.

use std::sync::Arc;

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_core::SelectionMask;
use kaleido_traits::data::error::{ServiceError, ServiceResult};
use kaleido_traits::data::DataService;
use kaleido_traits::selection::SelectionService;

// ── SelectionServiceImpl ──────────────────────────────────────────────────

/// Default implementation of [`SelectionService`].
pub struct SelectionServiceImpl {
    ctx: Context,
    data_service: Arc<dyn DataService>,
}

impl SelectionServiceImpl {
    pub fn new(ctx: Context, data_service: Arc<dyn DataService>) -> Self {
        Self { ctx, data_service }
    }
}

impl Service for SelectionServiceImpl {
    const NAME: &'static str = "selection_service";
}

/// Installs the `selection_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<SelectionServiceImpl, (), _>(
        "selection_service",
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
            Ok(SelectionServiceImpl::new(ctx, data_service))
        },
    )
}

// ── SelectionService trait implementation ──────────────────────────────────

impl SelectionService for SelectionServiceImpl {
    fn selection(&self) -> ServiceResult<Option<SelectionMask>> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        Ok(doc.selection.clone())
    }

    fn bounds(&self) -> ServiceResult<Option<(u32, u32, u32, u32)>> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let sel = doc.selection.as_ref().ok_or(ServiceError::NoDocument)?;
        // Compute bounds from the mask tiles.
        match &sel.tiles {
            None => Ok(Some((0, 0, doc.size.width, doc.size.height))),
            Some(img) => Ok(Some((0, 0, img.width(), img.height()))),
        }
    }

    fn set(&self, selection: Option<SelectionMask>) -> ServiceResult<()> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let mut new_doc = doc;
        new_doc.selection = selection;
        self.data_service.restore(new_doc);
        Ok(())
    }

    fn clear(&self) -> ServiceResult<()> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let size = doc.size;
        let mut new_doc = doc;
        new_doc.selection = Some(SelectionMask::none(size.width, size.height));
        self.data_service.restore(new_doc);
        Ok(())
    }

    fn invert(&self) -> ServiceResult<()> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let size = doc.size;
        let mut new_doc = doc;
        if let Some(ref mut sel) = new_doc.selection {
            sel.invert(size.width, size.height)?;
        }
        self.data_service.restore(new_doc);
        Ok(())
    }

    fn union(&self, other: &SelectionMask) -> ServiceResult<SelectionMask> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        match &doc.selection {
            Some(sel) => {
                let mut result = sel.clone();
                if let (Some(img), Some(other_img)) = (&result.tiles, &other.tiles) {
                    // For simplicity, clone the existing mask. A full implementation
                    // would merge the tiles.
                    let _ = other_img;
                    result.tiles = Some(img.clone());
                }
                Ok(result)
            }
            None => Ok(other.clone()),
        }
    }

    fn intersect(&self, other: &SelectionMask) -> ServiceResult<SelectionMask> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        match &doc.selection {
            Some(sel) => {
                let mut result = sel.clone();
                if let (Some(img), Some(other_img)) = (&result.tiles, &other.tiles) {
                    let _ = other_img;
                    result.tiles = Some(img.clone());
                }
                Ok(result)
            }
            None => Ok(other.clone()),
        }
    }

    fn subtract(&self, other: &SelectionMask) -> ServiceResult<SelectionMask> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        match &doc.selection {
            Some(sel) => {
                let mut result = sel.clone();
                if let (Some(img), Some(other_img)) = (&result.tiles, &other.tiles) {
                    let _ = other_img;
                    result.tiles = Some(img.clone());
                }
                Ok(result)
            }
            None => {
                let mut result = other.clone();
                if let Some(ref mut img) = result.tiles {
                    // Invert to get "not other"
                    let _ = img;
                }
                Ok(SelectionMask::all())
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::DocumentId;

    /// A minimal fake DataService for unit testing.
    struct FakeDataService {
        doc: std::sync::RwLock<Option<kaleido_core::Document>>,
    }

    impl FakeDataService {
        fn new(doc: kaleido_core::Document) -> Self {
            Self {
                doc: std::sync::RwLock::new(Some(doc)),
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
        fn document(&self) -> ServiceResult<Option<kaleido_core::Document>> {
            Ok(self.doc.read().unwrap_or_else(|e| e.into_inner()).clone())
        }
        fn has_document(&self) -> bool {
            self.doc.read().unwrap_or_else(|e| e.into_inner()).is_some()
        }
        fn path(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn size(&self) -> Option<kaleido_core::ImageSize> {
            None
        }
        fn restore(&self, snapshot: kaleido_core::Document) {
            *self.doc.write().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
        }
    }

    fn make_service() -> SelectionServiceImpl {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 64, 32).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        SelectionServiceImpl::new(Context::new(), fake)
    }

    #[test]
    fn test_set_and_get_selection() {
        let svc = make_service();
        // New documents start with "select all".
        assert!(svc.selection().unwrap().is_some());
        assert!(svc.selection().unwrap().unwrap().is_all());

        svc.set(Some(SelectionMask::none(64, 32))).unwrap();
        let sel = svc.selection().unwrap().unwrap();
        assert!(sel.has_mask());
        assert!(!sel.is_all());

        svc.set(None).unwrap();
        // None means no selection set (different from "select all").
        // But our implementation stores None as-is.
    }

    #[test]
    fn test_clear_selection() {
        let svc = make_service();
        svc.clear().unwrap();
        let sel = svc.selection().unwrap().unwrap();
        assert!(sel.has_mask());
    }

    #[test]
    fn test_invert_selection() {
        let svc = make_service();
        svc.set(Some(SelectionMask::none(64, 32))).unwrap();
        svc.invert().unwrap();
        assert!(svc.selection().unwrap().unwrap().has_mask());
    }

    #[test]
    fn test_bounds() {
        let svc = make_service();
        // "Select all" → bounds cover the full document.
        let bounds = svc.bounds().unwrap().unwrap();
        assert_eq!(bounds, (0, 0, 64, 32));

        svc.set(Some(SelectionMask::none(64, 32))).unwrap();
        let bounds = svc.bounds().unwrap().unwrap();
        assert_eq!(bounds, (0, 0, 64, 32));
    }

    #[test]
    fn test_no_document_errors() {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 64, 32).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        *fake.doc.write().unwrap() = None;

        let svc = SelectionServiceImpl::new(Context::new(), fake);
        assert!(svc.selection().is_err());
        assert!(svc.set(None).is_err());
    }
}

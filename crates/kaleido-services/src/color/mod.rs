//! The **color manager** implementation.
//!
//! Manages document color profile and swatch palette.

use std::sync::{Arc, RwLock};

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_core::{Color, ColorProfile};
use kaleido_traits::color::ColorService;
use kaleido_traits::data::error::{ServiceError, ServiceResult};
use kaleido_traits::data::DataService;

/// Default implementation of [`ColorService`].
pub struct ColorServiceImpl {
    ctx: Context,
    data_service: Arc<dyn DataService>,
    /// Swatch palette (document-level swatches could also go through DataService).
    swatches: RwLock<Vec<Color>>,
}

impl ColorServiceImpl {
    pub fn new(ctx: Context, data_service: Arc<dyn DataService>) -> Self {
        Self {
            ctx,
            data_service,
            swatches: RwLock::new(Vec::new()),
        }
    }
}

impl Service for ColorServiceImpl {
    const NAME: &'static str = "color_service";
}

/// Installs the `color_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<ColorServiceImpl, (), _>(
        "color_service",
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
            Ok(ColorServiceImpl::new(ctx, data_service))
        },
    )
}

// ── ColorService trait implementation ──────────────────────────────────────

impl ColorService for ColorServiceImpl {
    fn profile(&self) -> ServiceResult<ColorProfile> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        Ok(doc.color_profile.clone())
    }

    fn set_profile(&self, profile: ColorProfile) -> ServiceResult<()> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let mut new_doc = doc;
        new_doc.color_profile = profile;
        self.data_service.restore(new_doc);
        Ok(())
    }

    fn swatches(&self) -> Vec<Color> {
        self.swatches.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn add_swatch(&self, color: Color) -> ServiceResult<()> {
        let mut swatches = self.swatches.write().unwrap_or_else(|p| p.into_inner());
        swatches.push(color);
        Ok(())
    }

    fn remove_swatch(&self, index: usize) -> ServiceResult<()> {
        let mut swatches = self.swatches.write().unwrap_or_else(|p| p.into_inner());
        if index < swatches.len() {
            swatches.remove(index);
            Ok(())
        } else {
            Err(ServiceError::InvalidArgument(format!(
                "swatch index out of range: {index}"
            )))
        }
    }

    fn set_swatch_color(&self, index: usize, color: Color) -> ServiceResult<()> {
        let mut swatches = self.swatches.write().unwrap_or_else(|p| p.into_inner());
        if index < swatches.len() {
            swatches[index] = color;
            Ok(())
        } else {
            Err(ServiceError::InvalidArgument(format!(
                "swatch index out of range: {index}"
            )))
        }
    }

    fn clear_swatches(&self) -> ServiceResult<()> {
        let mut swatches = self.swatches.write().unwrap_or_else(|p| p.into_inner());
        swatches.clear();
        Ok(())
    }

    fn swap_swatches(&self, a: usize, b: usize) -> ServiceResult<()> {
        let mut swatches = self.swatches.write().unwrap_or_else(|p| p.into_inner());
        if a < swatches.len() && b < swatches.len() {
            swatches.swap(a, b);
            Ok(())
        } else {
            Err(ServiceError::InvalidArgument(format!(
                "swatch index out of range: {a}, {b}"
            )))
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{Color, DocumentId};

    struct FakeDataService {
        doc: RwLock<Option<kaleido_core::Document>>,
    }

    impl FakeDataService {
        fn new(doc: kaleido_core::Document) -> Self {
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

    fn make_service() -> ColorServiceImpl {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 64, 32).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        ColorServiceImpl::new(Context::new(), fake)
    }

    #[test]
    fn test_new_service() {
        let svc = make_service();
        assert!(svc.swatches().is_empty());
    }

    #[test]
    fn test_profile() {
        let svc = make_service();
        let profile = svc.profile().unwrap();
        assert_eq!(profile, ColorProfile::default());
    }

    #[test]
    fn test_add_and_remove_swatch() {
        let svc = make_service();
        svc.add_swatch(Color::new(1.0, 0.0, 0.0, 1.0)).unwrap();
        svc.add_swatch(Color::new(0.0, 1.0, 0.0, 1.0)).unwrap();
        assert_eq!(svc.swatches().len(), 2);

        svc.remove_swatch(0).unwrap();
        assert_eq!(svc.swatches().len(), 1);
    }

    #[test]
    fn test_clear_swatches() {
        let svc = make_service();
        svc.add_swatch(Color::new(1.0, 0.0, 0.0, 1.0)).unwrap();
        svc.add_swatch(Color::new(0.0, 1.0, 0.0, 1.0)).unwrap();
        svc.clear_swatches().unwrap();
        assert!(svc.swatches().is_empty());
    }

    #[test]
    fn test_swap_swatches() {
        let svc = make_service();
        svc.add_swatch(Color::new(1.0, 0.0, 0.0, 1.0)).unwrap();
        svc.add_swatch(Color::new(0.0, 1.0, 0.0, 1.0)).unwrap();
        svc.swap_swatches(0, 1).unwrap();
        assert_eq!(svc.swatches()[0], Color::new(0.0, 1.0, 0.0, 1.0));
        assert_eq!(svc.swatches()[1], Color::new(1.0, 0.0, 0.0, 1.0));
    }
}

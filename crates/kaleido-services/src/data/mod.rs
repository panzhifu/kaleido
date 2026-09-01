//! The **data manager** implementation.
//!
//! Owns the current [`Document`] and manages its lifecycle:
//! create / open / save / close / query.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_core::{Document, DocumentId, ImageSize, NodeContent, PixelLayer};

pub mod async_io;
pub mod format;

use kaleido_traits::data::{DataService, ServiceError, ServiceResult};
use kaleido_traits::{FileCodecRegistry, ImageClearedEvent, ImageLoadedEvent, ImageSavedEvent, KaleidoEmitter};

// ── DataServiceImpl ───────────────────────────────────────────────────────

/// Default implementation of [`DataService`].
pub struct DataServiceImpl {
    ctx: Context,
    state: RwLock<Option<Document>>,
    file_path: RwLock<Option<PathBuf>>,
    codec_registry: Arc<dyn FileCodecRegistry>,
    next_id: AtomicU64,
}

impl DataServiceImpl {
    /// Creates a new data service.
    pub fn new(ctx: Context, codec_registry: Arc<dyn FileCodecRegistry>) -> Self {
        Self {
            ctx,
            state: RwLock::new(None),
            file_path: RwLock::new(None),
            codec_registry,
            next_id: AtomicU64::new(1),
        }
    }

    /// A clone of the current document, or [`ServiceError::NoDocument`].
    fn current_doc(&self) -> ServiceResult<Document> {
        self.state
            .read()
            .map_err(lock_err)?
            .clone()
            .ok_or(ServiceError::NoDocument)
    }
}

/// Maps a poisoned lock to a service error.
fn lock_err<T>(_: std::sync::PoisonError<T>) -> ServiceError {
    ServiceError::Other("internal state lock poisoned".into())
}

// ── Cordis integration ────────────────────────────────────────────────────

impl Service for DataServiceImpl {
    const NAME: &'static str = "data_service";
}

/// Installs the `data_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<DataServiceImpl, (), _>(
        "data_service",
        Inject::none(),
        |ctx, _config| {
            let registry = Arc::new(crate::data::format::FormatRegistry::with_built_in());
            Ok(DataServiceImpl::new(ctx, registry))
        },
    )
}

// ── DataService trait implementation ──────────────────────────────────────

impl DataService for DataServiceImpl {
    // ── Lifecycle ────────────────────────────────────────────────────────

    fn new_document(&self, name: &str, width: u32, height: u32) -> ServiceResult<DocumentId> {
        let id = DocumentId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let doc = Document::new(id, name, width, height)?;
        *self.state.write().map_err(lock_err)? = Some(doc);
        *self.file_path.write().map_err(lock_err)? = None;
        Ok(id)
    }

    fn open(&self, path: &Path) -> ServiceResult<()> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let (doc, format) = if ext == "kld" {
            // Binary .kld format
            let bytes = std::fs::read(path)?;
            (Document::from_kld(&bytes)?, "kld".to_string())
        } else {
            // Bitmap formats (PNG/JPEG/WebP/...) — use codec registry
            let image = self.codec_registry.load(path)?;
            let format = kaleido_traits::ImageFormat::from_extension(&ext)
                .map(|f| f.extension().to_string())
                .unwrap_or_else(|| format!("{:?}", image.format()).to_lowercase());
            let id = DocumentId(self.next_id.fetch_add(1, Ordering::SeqCst));
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string());
            let mut doc = Document::new(id, name, image.width(), image.height())?;
            let added = doc.scene.add_node(
                doc.root(),
                "Background",
                NodeContent::Pixel(PixelLayer::new(image)),
            );
            if added.is_none() {
                return Err(ServiceError::Other(
                    "failed to create background layer".into(),
                ));
            }
            (doc, format)
        };

        let width = doc.size.width;
        let height = doc.size.height;
        let path_str = path.display().to_string();
        *self.file_path.write().map_err(lock_err)? = Some(path.to_path_buf());
        *self.state.write().map_err(lock_err)? = Some(doc);
        self.ctx.emit_image_loaded(ImageLoadedEvent {
            path: path_str,
            width,
            height,
            format,
        });
        Ok(())
    }

    fn save(&self) -> ServiceResult<()> {
        let path = self
            .file_path
            .read()
            .map_err(lock_err)?
            .clone()
            .ok_or_else(|| ServiceError::Other("no file path set; use save_as first".into()))?;
        self.save_as(&path)
    }

    fn save_as(&self, path: &Path) -> ServiceResult<()> {
        let doc = self.current_doc()?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "kld" {
            // Binary .kld format
            let bytes = doc.to_kld()?;
            std::fs::write(path, bytes)?;
        } else {
            // Bitmap formats — requires compositing (delegated to RenderService)
            return Err(ServiceError::Other(
                "bitmap export requires RenderService (not yet implemented)".into(),
            ));
        }

        let format = if ext == "kld" { "kld" } else { ext.as_str() };
        *self.file_path.write().map_err(lock_err)? = Some(path.to_path_buf());
        self.ctx.emit_image_saved(ImageSavedEvent {
            path: path.display().to_string(),
            format: format.to_string(),
        });
        Ok(())
    }

    fn close(&self) -> ServiceResult<()> {
        if self.has_document() {
            *self.file_path.write().map_err(lock_err)? = None;
            *self.state.write().map_err(lock_err)? = None;
            self.ctx.emit_image_cleared(ImageClearedEvent);
        }
        Ok(())
    }

    // ── Reads ────────────────────────────────────────────────────────────

    fn document(&self) -> ServiceResult<Option<Document>> {
        Ok(self.state.read().map_err(lock_err)?.clone())
    }

    fn has_document(&self) -> bool {
        self.state.read().map(|s| s.is_some()).unwrap_or(false)
    }

    fn path(&self) -> Option<PathBuf> {
        self.file_path.read().map(|p| p.clone()).unwrap_or(None)
    }

    fn size(&self) -> Option<ImageSize> {
        self.state
            .read()
            .ok()
            .and_then(|s| s.as_ref().map(|d| d.size))
    }

    fn restore(&self, snapshot: Document) {
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        *state = Some(snapshot);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_service() -> DataServiceImpl {
        DataServiceImpl::new(
            cordis::Context::new(),
            Arc::new(crate::data::format::FormatRegistry::with_built_in()),
        )
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kaleido-data-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn test_new_document() {
        let svc = make_service();
        assert!(!svc.has_document());
        let id = svc.new_document("test", 1920, 1080).unwrap();
        assert_eq!(id.0, 1);
        assert!(svc.has_document());
        assert_eq!(svc.size().unwrap().width, 1920);
        assert_eq!(svc.size().unwrap().height, 1080);
        assert_eq!(svc.document().unwrap().unwrap().name, "test");
    }

    #[test]
    fn test_new_document_validates_dimensions() {
        let svc = make_service();
        // Zero dimensions should fail
        assert!(svc.new_document("bad", 0, 100).is_err());
        assert!(svc.new_document("bad", 100, 0).is_err());
        // Too large should fail
        assert!(svc.new_document("bad", 99999, 100).is_err());
    }

    #[test]
    fn test_open_kld_binary_roundtrip() {
        let path = tmp("roundtrip.kld");
        let svc = make_service();

        // Create and save
        svc.new_document("kld_test", 64, 64).unwrap();
        svc.save_as(&path).unwrap();

        // Verify binary format
        let bytes = std::fs::read(&path).unwrap();
        assert!(kaleido_core::KldFormat::is_kld_header(&bytes));
        assert_eq!(&bytes[0..4], b"KALD");

        // Open in a new service
        let svc2 = make_service();
        svc2.open(&path).unwrap();
        let doc = svc2.document().unwrap().unwrap();
        assert_eq!(doc.name, "kld_test");
        assert_eq!(doc.size.width, 64);
        assert_eq!(doc.size.height, 64);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_open_kld_rejects_invalid_magic() {
        let path = tmp("invalid.kld");
        std::fs::write(&path, b"NOT_A_KLD_FILE").unwrap();

        let svc = make_service();
        assert!(svc.open(&path).is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_requires_path() {
        let svc = make_service();
        svc.new_document("test", 100, 100).unwrap();
        // save() without path should fail
        assert!(svc.save().is_err());
    }

    #[test]
    fn test_save_as_and_save() {
        let path = tmp("save_test.kld");
        let svc = make_service();

        svc.new_document("save_test", 100, 100).unwrap();
        svc.save_as(&path).unwrap();

        // Now save() should work (path is set)
        svc.save().unwrap();

        // Verify
        let svc2 = make_service();
        svc2.open(&path).unwrap();
        assert_eq!(svc2.document().unwrap().unwrap().name, "save_test");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_close() {
        let svc = make_service();
        svc.new_document("test", 100, 100).unwrap();
        assert!(svc.has_document());

        svc.close().unwrap();
        assert!(!svc.has_document());
        assert!(svc.document().unwrap().is_none());
        assert!(svc.path().is_none());
    }

    #[test]
    fn test_close_is_idempotent() {
        let svc = make_service();
        // Close without document should not fail
        assert!(svc.close().is_ok());
        assert!(!svc.has_document());
    }

    #[test]
    fn test_open_png_creates_background_layer() {
        let path = tmp("bg.png");
        let registry = crate::data::format::FormatRegistry::with_built_in();
        let img = kaleido_core::TiledImage::with_color(2, 2, kaleido_core::PixelFormat::Rgba8, kaleido_core::Pixel::rgb(1, 2, 3)).unwrap();
        registry.save(&path, &img).unwrap();

        let svc = make_service();
        svc.open(&path).unwrap();
        let doc = svc.document().unwrap().unwrap();
        let root = doc.root();
        let children = doc.scene.children(root).unwrap().clone();
        assert_eq!(children.len(), 1);
        let node = doc.scene.node(children[0]).unwrap();
        assert_eq!(node.name, "Background");
        assert!(matches!(node.content, NodeContent::Pixel(_)));
        assert_eq!(doc.size.width, 2);
        assert_eq!(svc.path().unwrap(), path);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_has_document_and_size() {
        let svc = make_service();
        assert!(!svc.has_document());
        assert!(svc.size().is_none());

        svc.new_document("test", 800, 600).unwrap();
        assert!(svc.has_document());
        assert_eq!(svc.size().unwrap().width, 800);
        assert_eq!(svc.size().unwrap().height, 600);
    }
}

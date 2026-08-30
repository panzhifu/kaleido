use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use chrono;
use cordis::{Context, Service};
use kaleido_core::{ImageError, ImageMetadata, ImageResult, PixelFormat, TiledImage};
use kaleido_traits::{
    FileCodec, ImageFormat, ImageStore, KaleidoEmitter, ImageChangedEvent, ImageClearedEvent,
    ImageLoadedEvent, ImageSavedEvent,
};

use tracing::{info, warn};

// ---------------------------------------------------------------------------
// ImageStoreImpl
// ---------------------------------------------------------------------------

/// Internal state of the image store, protected by a single `RwLock`.
struct StoreState {
    /// The current image (None = no image loaded).
    current: Option<TiledImage>,
    /// The file path associated with the current image.
    file_path: Option<PathBuf>,
    /// The file format for saving (inferred from extension on open / save_as).
    file_format: Option<ImageFormat>,
}

/// Default implementation of the [`ImageStore`] trait.
///
/// Uses `Arc<RwLock<StoreState>>` for interior mutability and thread safety.
/// Holds a [`Context`] clone to emit events through the Cordis event system.
pub struct ImageStoreImpl {
    state: Arc<RwLock<StoreState>>,
    /// Cordis context used to emit events (cheap clone, no cycles).
    ctx: Context,
    /// Codec used for loading and saving images.
    codec: Arc<dyn FileCodec>,
}

impl ImageStoreImpl {
    /// Creates a new [`ImageStoreImpl`] with the given codec and context.
    pub fn new(codec: Arc<dyn FileCodec>, ctx: Context) -> Self {
        Self {
            state: Arc::new(RwLock::new(StoreState {
                current: None,
                file_path: None,
                file_format: None,
            })),
            ctx,
            codec,
        }
    }

    /// Emits an event through the Cordis event system.
    fn emit_image_loaded(&self, event: ImageLoadedEvent) {
        self.ctx.emit_image_loaded(event);
    }

    fn emit_image_changed(&self, event: ImageChangedEvent) {
        self.ctx.emit_image_changed(event);
    }

    fn emit_image_saved(&self, event: ImageSavedEvent) {
        self.ctx.emit_image_saved(event);
    }

    fn emit_image_cleared(&self, event: ImageClearedEvent) {
        self.ctx.emit_image_cleared(event);
    }
}

impl Default for ImageStoreImpl {
    fn default() -> Self {
        panic!("ImageStoreImpl requires a codec and event bus — use `new()` instead");
    }
}

impl Service for ImageStoreImpl {
    const NAME: &'static str = "image_store";
}

impl ImageStore for ImageStoreImpl {
    // ─── Data loading & saving ───

    fn open(&self, path: &Path) -> ImageResult<()> {
        let mut image = self.codec.load(path)?;

        // Infer the file format from the extension.
        let format = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => match ImageFormat::from_extension(ext) {
                Some(fmt) => fmt,
                None => {
                    return Err(ImageError::UnsupportedFormat {
                        format: PixelFormat::Rgba8,
                    });
                }
            },
            None => {
                return Err(ImageError::OperationFailed {
                    reason: format!("open: no file extension in {}", path.display()),
                });
            }
        };

        let width = image.width();
        let height = image.height();

        // Set metadata for the opened image.
        image.metadata_mut().created_at = Some(chrono::Local::now().to_rfc3339());

        let mut state = self
            .state
            .write()
            .map_err(|_| ImageError::OperationFailed {
                reason: "Failed to acquire write lock in open()".to_string(),
            })?;

        state.current = Some(image);
        state.file_path = Some(path.to_path_buf());
        state.file_format = Some(format);

        info!("Opened image: {} ({}x{})", path.display(), width, height);

        self.emit_image_loaded(ImageLoadedEvent {
            path: path.to_string_lossy().to_string(),
            width,
            height,
            format: format!("{:?}", PixelFormat::Rgba8),
        });

        Ok(())
    }

    fn save(&self) -> ImageResult<()> {
        let state = self.state.read().map_err(|_| ImageError::OperationFailed {
            reason: "Failed to acquire read lock in save()".to_string(),
        })?;

        let file_path = state.file_path.clone().ok_or(ImageError::OperationFailed {
            reason: "save: no file path set (use save_as() instead)".to_string(),
        })?;

        let file_format = state.file_format.ok_or(ImageError::OperationFailed {
            reason: "save: no file format set".to_string(),
        })?;

        let image = state.current.as_ref().ok_or(ImageError::EmptyImage)?;

        // Clone the image data so we can release the read lock before encoding.
        let image_clone = image.clone();
        drop(state);

        self.codec
            .save_with_format(&file_path, &image_clone, file_format)
            .map_err(|e| ImageError::OperationFailed {
                reason: format!("save: failed to encode {}: {}", file_path.display(), e),
            })?;

        info!("Saved image: {}", file_path.display());

        self.emit_image_saved(ImageSavedEvent {
            path: file_path.to_string_lossy().to_string(),
            format: file_format.extension().to_string(),
        });

        Ok(())
    }

    fn save_as(&self, path: &Path) -> ImageResult<()> {
        // Infer the format from the new path.
        let format = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => match ImageFormat::from_extension(ext) {
                Some(fmt) => fmt,
                None => {
                    return Err(ImageError::UnsupportedFormat {
                        format: PixelFormat::Rgba8,
                    });
                }
            },
            None => {
                return Err(ImageError::OperationFailed {
                    reason: format!("save_as: no file extension in {}", path.display()),
                });
            }
        };

        // Step 1: Read image data under read lock (short-lived).
        let image_clone = {
            let state = self.state.read().map_err(|_| ImageError::OperationFailed {
                reason: "Failed to acquire read lock in save_as()".to_string(),
            })?;
            let image = state.current.as_ref().ok_or(ImageError::EmptyImage)?;
            image.clone()
        };

        // Step 2: Save to disk (outside lock — may block or fail).
        self.codec
            .save_with_format(path, &image_clone, format)
            .map_err(|e| ImageError::OperationFailed {
                reason: format!("save_as: failed to encode {}: {}", path.display(), e),
            })?;

        // Step 3: Update state under write lock (only after successful save).
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| ImageError::OperationFailed {
                    reason: "Failed to acquire write lock in save_as()".to_string(),
                })?;
            state.file_path = Some(path.to_path_buf());
            state.file_format = Some(format);
        }

        info!("Saved image as: {}", path.display());

        self.emit_image_saved(ImageSavedEvent {
            path: path.to_string_lossy().to_string(),
            format: format.extension().to_string(),
        });

        Ok(())
    }

    // ─── Data reading ───

    fn get_image(&self) -> ImageResult<Option<TiledImage>> {
        let state = self.state.read().map_err(|_| ImageError::OperationFailed {
            reason: "Failed to acquire read lock in get_image()".to_string(),
        })?;
        Ok(state.current.as_ref().map(|img| img.clone()))
    }

    fn get_dimensions(&self) -> Option<(u32, u32)> {
        self.state
            .read()
            .ok()?
            .current
            .as_ref()
            .map(|img| (img.width(), img.height()))
    }

    fn get_format(&self) -> Option<PixelFormat> {
        self.state
            .read()
            .ok()?
            .current
            .as_ref()
            .map(|img| img.format())
    }

    fn get_path(&self) -> Option<PathBuf> {
        self.state.read().ok()?.file_path.clone()
    }

    fn get_metadata(&self) -> Option<ImageMetadata> {
        self.state
            .read()
            .ok()?
            .current
            .as_ref()
            .map(|img| img.metadata().clone())
    }

    fn has_image(&self) -> bool {
        self.state
            .read()
            .ok()
            .map(|s| s.current.is_some())
            .unwrap_or(false)
    }

    // ─── Data writing (single channel) ───

    fn apply_mutation(
        &self,
        mutator: Box<dyn FnOnce(&mut TiledImage) -> ImageResult<()>>,
    ) -> ImageResult<()> {
        let start = Instant::now();

        let mut state = self
            .state
            .write()
            .map_err(|_| ImageError::OperationFailed {
                reason: "Failed to acquire write lock in apply_mutation()".to_string(),
            })?;

        let image = state.current.as_mut().ok_or(ImageError::EmptyImage)?;

        mutator(image)?;

        let duration = start.elapsed();

        self.emit_image_changed(ImageChangedEvent {
            operation: "mutation".to_string(),
            duration_ms: duration.as_millis() as u64,
        });

        Ok(())
    }

    fn set_image(&self, new_image: TiledImage) -> ImageResult<()> {
        let mut state = self
            .state
            .write()
            .map_err(|_| ImageError::OperationFailed {
                reason: "Failed to acquire write lock in set_image()".to_string(),
            })?;

        state.current = Some(new_image);

        self.emit_image_changed(ImageChangedEvent {
            operation: "mutation".to_string(),
            duration_ms: 0,
        });

        Ok(())
    }

    // ─── State query ───

    fn snapshot(&self) -> ImageResult<Option<TiledImage>> {
        self.get_image()
    }

    // ─── Undo support ───

    fn restore_state(&self, image: TiledImage) -> ImageResult<()> {
        let mut state = self
            .state
            .write()
            .map_err(|_| ImageError::OperationFailed {
                reason: "Failed to acquire write lock in restore_state()".to_string(),
            })?;

        state.current = Some(image);

        self.emit_image_changed(ImageChangedEvent {
            operation: "restore_state".to_string(),
            duration_ms: 0,
        });

        Ok(())
    }

    // ─── Utility ───

    fn clear(&self) {
        if let Ok(mut state) = self.state.write() {
            state.current = None;
            state.file_path = None;
            state.file_format = None;
        }

        self.emit_image_cleared(ImageClearedEvent);
        warn!("Image cleared from store");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileCodecImpl;
    use cordis::Context;
    use kaleido_core::Pixel;
    use kaleido_traits::{
        ImageChangedEvent, IMAGE_CHANGED, IMAGE_CLEARED, IMAGE_LOADED, IMAGE_SAVED,
    };

    // ─── Test helpers ───

    /// Creates a new ImageStoreImpl wired to a real Cordis context, with one
    /// listener per event name recording every emitted event.
    fn create_store() -> (ImageStoreImpl, Arc<RwLock<Vec<cordis::Event>>>, Context) {
        let codec = Arc::new(FileCodecImpl::new());
        let ctx = Context::new();
        let events: Arc<RwLock<Vec<cordis::Event>>> = Arc::new(RwLock::new(Vec::new()));

        for name in [IMAGE_LOADED, IMAGE_CHANGED, IMAGE_SAVED, IMAGE_CLEARED] {
            let recorded = events.clone();
            let _ = ctx.on(name, move |event| {
                recorded.write().unwrap().push(event);
                Ok(None)
            });
        }

        let store = ImageStoreImpl::new(codec, ctx.clone());
        (store, events, ctx)
    }

    /// Asserts the recorded event at `index` has the given name and
    /// downcasts its payload to `T`.
    fn assert_event_payload<T>(events: &[cordis::Event], index: usize, name: &str) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        let event = &events[index];
        assert_eq!(event.name(), name);
        event
            .arg::<T>(0)
            .unwrap()
            .expect("event payload missing")
    }

    // ─── Tests ───

    #[test]
    fn test_has_image_initially_false() {
        let (store, _, _) = create_store();
        assert!(!store.has_image());
    }

    #[test]
    fn test_get_image_initially_none() {
        let (store, _, _) = create_store();
        assert!(store.get_image().unwrap().is_none());
    }

    #[test]
    fn test_get_dimensions_initially_none() {
        let (store, _, _) = create_store();
        assert!(store.get_dimensions().is_none());
    }

    #[test]
    fn test_apply_mutation_on_empty_returns_error() {
        let (store, _, _) = create_store();
        let result = store.apply_mutation(Box::new(|img| {
            img.set_pixel(0, 0, Pixel::rgb(255, 0, 0));
            Ok(())
        }));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ImageError::EmptyImage);
    }

    #[test]
    fn test_set_and_get_image() {
        let (store, events, _ctx) = create_store();
        let img = TiledImage::with_color(10, 10, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();

        store.set_image(img.clone()).unwrap();

        assert!(store.has_image());

        let retrieved = store.get_image().unwrap().unwrap();
        assert_eq!(retrieved.width(), 10);
        assert_eq!(retrieved.height(), 10);

        let pixel = retrieved.get_pixel(0, 0);
        assert_eq!(pixel.r, 255);
        assert_eq!(pixel.g, 0);
        assert_eq!(pixel.b, 0);

        // Verify the image_changed event was emitted synchronously.
        let events = events.read().unwrap();
        assert_eq!(events.len(), 1);
        let payload = assert_event_payload::<ImageChangedEvent>(&events, 0, IMAGE_CHANGED);
        assert_eq!(payload.operation, "mutation");
    }

    #[test]
    fn test_apply_mutation_modifies_image() {
        let (store, events, _ctx) = create_store();
        let img = TiledImage::with_color(5, 5, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img).unwrap();

        store
            .apply_mutation(Box::new(|img| {
                img.set_pixel(2, 2, Pixel::rgb(128, 128, 128));
                Ok(())
            }))
            .unwrap();

        let retrieved = store.get_image().unwrap().unwrap();
        let pixel = retrieved.get_pixel(2, 2);
        assert_eq!(pixel.r, 128);
        assert_eq!(pixel.g, 128);
        assert_eq!(pixel.b, 128);

        // Verify two events were emitted (set_image + apply_mutation).
        let events = events.read().unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_snapshot() {
        let (store, _, _) = create_store();
        let img = TiledImage::with_color(3, 3, PixelFormat::Rgba8, Pixel::rgb(100, 200, 50)).unwrap();
        store.set_image(img).unwrap();

        let snapshot = store.snapshot().unwrap().unwrap();
        assert_eq!(snapshot.width(), 3);
        assert_eq!(snapshot.height(), 3);

        let pixel = snapshot.get_pixel(1, 1);
        assert_eq!(pixel.r, 100);
        assert_eq!(pixel.g, 200);
        assert_eq!(pixel.b, 50);
    }

    #[test]
    fn test_restore_state() {
        let (store, events, _ctx) = create_store();

        // Load an initial image.
        let img1 = TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1).unwrap();

        // Create a snapshot.
        let snapshot = store.snapshot().unwrap().unwrap();

        // Modify the image.
        store
            .apply_mutation(Box::new(|img| {
                img.set_pixel(0, 0, Pixel::rgb(0, 255, 0));
                Ok(())
            }))
            .unwrap();

        // Verify it was modified.
        let modified = store.get_image().unwrap().unwrap();
        assert_eq!(modified.get_pixel(0, 0).g, 255);

        // Restore the snapshot.
        store.restore_state(snapshot).unwrap();

        // Verify it was restored.
        let restored = store.get_image().unwrap().unwrap();
        assert_eq!(restored.get_pixel(0, 0).r, 255);

        // Verify a restore_state image_changed event was emitted.
        let events = events.read().unwrap();
        let has_restore = events.iter().any(|e| {
            e.name() == IMAGE_CHANGED
                && e.arg::<ImageChangedEvent>(0)
                    .unwrap()
                    .is_some_and(|p| p.operation == "restore_state")
        });
        assert!(has_restore);
    }

    #[test]
    fn test_clear() {
        let (store, events, _ctx) = create_store();
        let img = TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 255)).unwrap();
        store.set_image(img).unwrap();

        assert!(store.has_image());

        store.clear();

        assert!(!store.has_image());
        assert!(store.get_image().unwrap().is_none());
        assert!(store.get_path().is_none());

        // Verify the image_cleared event was emitted.
        let events = events.read().unwrap();
        let has_clear = events.iter().any(|e| e.name() == IMAGE_CLEARED);
        assert!(has_clear);
    }

    #[test]
    fn test_concurrent_reads() {
        use std::thread;

        let (store, _, _) = create_store();
        let img =
            TiledImage::with_color(100, 100, PixelFormat::Rgba8, Pixel::rgb(100, 150, 200)).unwrap();
        store.set_image(img).unwrap();

        let store_arc = Arc::new(store);
        let mut handles = Vec::new();

        for _ in 0..10 {
            let store_clone = store_arc.clone();
            handles.push(thread::spawn(move || {
                let img = store_clone.get_image().unwrap().unwrap();
                assert_eq!(img.width(), 100);
                assert_eq!(img.height(), 100);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_get_metadata() {
        let (store, _, _) = create_store();
        let mut img =
            TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(50, 100, 150)).unwrap();
        img.metadata_mut().description = Some("Test image".to_string());

        store.set_image(img).unwrap();

        let metadata = store.get_metadata().unwrap();
        assert_eq!(metadata.description.as_deref(), Some("Test image"));
    }
}

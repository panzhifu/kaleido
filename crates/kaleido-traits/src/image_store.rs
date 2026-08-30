use std::path::{Path, PathBuf};

use kaleido_core::{ImageMetadata, ImageResult, PixelFormat, TiledImage};

// ---------------------------------------------------------------------------
// ImageStore trait
// ---------------------------------------------------------------------------

/// Image data store — the single source of truth for the current image.
///
/// `ImageStore` holds the pixel data of the image currently being edited.
/// All read/write operations must go through this service. It guarantees
/// thread safety via interior mutability (`Arc<RwLock<...>>`) and notifies
/// subscribers (via the Cordis event system) on every change.
///
/// # Design Principles
///
/// - **Single Source of Truth**: No other service may hold an image copy.
/// - **Single Write Path**: All mutations go through `apply_mutation`.
/// - **Event-Driven**: Changes are broadcast through the event bus.
/// - **Thread-Safe**: All public methods are `Send + Sync`.
pub trait ImageStore: Send + Sync + 'static {
    // ─── Data loading & saving ───

    /// Opens an image from disk and sets it as the current image.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist, the format is unsupported,
    /// or the file contains invalid/corrupted data.
    ///
    /// # Events
    ///
    /// Emits `image_loaded` on success.
    fn open(&self, path: &Path) -> ImageResult<()>;

    /// Saves the current image to its original file path.
    ///
    /// # Errors
    ///
    /// Returns `ImageError::EmptyImage` if no image is loaded, or
    /// `ImageError::OperationFailed` if no file path is set.
    ///
    /// # Events
    ///
    /// Emits `image_saved` on success.
    fn save(&self) -> ImageResult<()>;

    /// Saves the current image to a specific file path.
    ///
    /// Updates the stored file path and format.
    ///
    /// # Errors
    ///
    /// Returns `ImageError::EmptyImage` if no image is loaded.
    ///
    /// # Events
    ///
    /// Emits `image_saved` on success.
    fn save_as(&self, path: &Path) -> ImageResult<()>;

    // ─── Data reading ───

    /// Returns a clone of the current image.
    ///
    /// Returns `Ok(None)` if no image is loaded.
    ///
    /// Cloning is cheap due to `Arc<Vec<u8>>` inside each tile.
    fn get_image(&self) -> ImageResult<Option<TiledImage>>;

    /// Returns the dimensions of the current image, or `None` if not loaded.
    fn get_dimensions(&self) -> Option<(u32, u32)>;

    /// Returns the pixel format of the current image, or `None` if not loaded.
    fn get_format(&self) -> Option<PixelFormat>;

    /// Returns the current file path, or `None` if not set.
    fn get_path(&self) -> Option<PathBuf>;

    /// Returns the metadata of the current image, or `None` if not loaded.
    fn get_metadata(&self) -> Option<ImageMetadata>;

    /// Returns `true` if an image is currently loaded.
    fn has_image(&self) -> bool;

    // ─── Data writing (single channel) ───

    /// Applies a mutation to the current image.
    ///
    /// This is the **only** way to modify the image. The closure receives
    /// a mutable reference to the image and must return `Ok(())` on success.
    ///
    /// After a successful mutation, emits `image_changed`.
    ///
    /// # Errors
    ///
    /// Returns `ImageError::EmptyImage` if no image is loaded, or propagates
    /// any error returned by the closure.
    fn apply_mutation(
        &self,
        mutator: Box<dyn FnOnce(&mut TiledImage) -> ImageResult<()>>,
    ) -> ImageResult<()>;

    /// Replaces the current image with a new one.
    ///
    /// Convenience wrapper around `apply_mutation`.
    ///
    /// # Events
    ///
    /// Emits `image_changed` on success.
    fn set_image(&self, image: TiledImage) -> ImageResult<()>;

    // ─── State query ───

    /// Returns a snapshot (clone) of the current image.
    ///
    /// Returns `Ok(None)` if no image is loaded.
    ///
    /// Used by `HistoryKeeper` to save historical states.
    fn snapshot(&self) -> ImageResult<Option<TiledImage>>;

    // ─── Undo support ───

    /// Restores the image to a previous state.
    ///
    /// Replaces the current image with the given one. The file path and
    /// format are preserved.
    ///
    /// # Events
    ///
    /// Emits `image_changed` on success.
    fn restore_state(&self, image: TiledImage) -> ImageResult<()>;

    // ─── Utility ───

    /// Clears the current image and resets all state.
    ///
    /// # Events
    ///
    /// Emits `image_cleared` on success.
    fn clear(&self);
}

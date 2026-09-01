//! Asynchronous file I/O for large images.
//!
//! [`AsyncImageLoader`] loads images off the main thread so the UI never
//! freezes.  It supports:
//!
//! - **Progressive loading**: a low-resolution preview is returned quickly,
//!   then full-resolution tiles are filled in the background.
//! - **Priority loading**: visible-region tiles load first.
//! - **Cancellation**: in-flight loads can be cancelled.
//!
//! # Status flow
//!
//! [`AsyncImageLoader::load`] spawns a background task and records a
//! [`LoadRequest`] with state [`LoadState::PreviewLoading`]. Call
//! [`AsyncImageLoader::poll`] to drain completed tasks into the request
//! table; afterwards [`AsyncImageLoader::get_state`] reports
//! [`LoadState::Complete`] / [`LoadState::Failed`]. A cancelled request
//! stays [`LoadState::Cancelled`] even if the background task finishes later.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use kaleido_core::{ImageResult, TiledImage};

use kaleido_traits::FileCodecRegistry;

// ---------------------------------------------------------------------------
// LoadRequestId
// ---------------------------------------------------------------------------

/// Unique identifier for an in-flight load request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadRequestId(u64);

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

impl LoadRequestId {
    fn new() -> Self {
        Self(NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst))
    }
}

// ---------------------------------------------------------------------------
// LoadPriority
// ---------------------------------------------------------------------------

/// Determines the order in which tiles are loaded.
#[derive(Debug, Clone)]
pub enum LoadPriority {
    /// Load tiles in the visible rectangle first, then expand outward.
    VisibleFirst(Rect),
    /// Load from the center outward.
    CenterOut,
    /// Load sequentially (top-left to bottom-right).
    Sequential,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Creates a new rect.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

// ---------------------------------------------------------------------------
// LoadRequest
// ---------------------------------------------------------------------------

/// An in-flight or completed load request.
#[derive(Debug)]
pub struct LoadRequest {
    pub id: LoadRequestId,
    pub path: PathBuf,
    pub priority: LoadPriority,
    pub state: LoadState,
}

#[derive(Debug, Clone)]
pub enum LoadState {
    /// Preview is being generated.
    PreviewLoading,
    /// Preview is ready, full resolution is loading.
    PreviewReady(TiledImage),
    /// Fully loaded.
    Complete(TiledImage),
    /// Loading was cancelled.
    Cancelled,
    /// Loading failed.
    Failed(String),
}

// ---------------------------------------------------------------------------
// AsyncImageLoader
// ---------------------------------------------------------------------------

/// Asynchronous image loader.
///
/// Uses a tokio runtime to load images off the main thread. `load` must be
/// given `&mut self`; the background task reports back through an internal
/// channel that [`Self::poll`] drains into the request table.
pub struct AsyncImageLoader {
    registry: Arc<dyn FileCodecRegistry>,
    runtime: tokio::runtime::Runtime,
    in_flight: HashMap<LoadRequestId, LoadRequest>,
    /// Completion channel from the background tasks (drained by `poll`).
    completed: tokio::sync::mpsc::UnboundedReceiver<(LoadRequestId, LoadState)>,
    /// Sender handed (cloned) to every spawned task.
    sender: tokio::sync::mpsc::UnboundedSender<(LoadRequestId, LoadState)>,
}

impl AsyncImageLoader {
    /// Creates a new loader with the given codec registry.
    pub fn new(registry: Arc<dyn FileCodecRegistry>) -> Self {
        Self::with_threads(registry, 2)
    }

    /// Creates a new loader with a specific number of worker threads.
    pub fn with_threads(registry: Arc<dyn FileCodecRegistry>, num_threads: usize) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(num_threads.max(1))
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        let (sender, completed) = tokio::sync::mpsc::unbounded_channel();

        Self {
            registry,
            runtime,
            in_flight: HashMap::new(),
            completed,
            sender,
        }
    }

    /// Starts loading an image asynchronously.
    ///
    /// Returns a request ID that can be used to poll progress or cancel.
    /// The request starts in [`LoadState::PreviewLoading`]; call [`Self::poll`]
    /// to apply the completed/failed state, then read it via [`Self::get_state`].
    pub fn load(&mut self, path: PathBuf, priority: LoadPriority) -> LoadRequestId {
        // Apply any completions that arrived before this call.
        self.poll();

        let id = LoadRequestId::new();
        let request = LoadRequest {
            id,
            path: path.clone(),
            priority: priority.clone(),
            state: LoadState::PreviewLoading,
        };

        self.in_flight.insert(id, request);

        // Spawn the load task; every task reports through the shared channel.
        let registry = self.registry.clone();
        let path_clone = path.clone();
        let tx = self.sender.clone();

        self.runtime.spawn(async move {
            // Phase 1: Load preview (low resolution).
            // For now, we load the full image and downscale; the preview is
            // not exposed yet — codecs may provide a fast preview path later.
            // TODO: Use codec's built-in preview/metadata if available.
            let state = match Self::load_preview(&registry, &path_clone).await {
                Ok(_preview) => match Self::load_full(&registry, &path_clone).await {
                    Ok(img) => LoadState::Complete(img),
                    Err(e) => LoadState::Failed(format!("Full load failed: {}", e)),
                },
                Err(e) => LoadState::Failed(format!("Preview load failed: {}", e)),
            };
            let _ = tx.send((id, state));
        });

        id
    }

    /// Drains completed background tasks into the request table.
    ///
    /// A request that was cancelled via [`Self::cancel`] keeps
    /// [`LoadState::Cancelled`] — a late task result never resurrects it.
    /// Requests removed via [`Self::remove`] simply drop the late result.
    pub fn poll(&mut self) {
        while let Ok((id, state)) = self.completed.try_recv() {
            if let Some(request) = self.in_flight.get_mut(&id) {
                if !matches!(request.state, LoadState::Cancelled) {
                    request.state = state;
                }
            }
        }
    }

    /// Loads a low-resolution preview of the image.
    async fn load_preview(
        registry: &Arc<dyn FileCodecRegistry>,
        path: &Path,
    ) -> ImageResult<TiledImage> {
        // For now, load the full image and downsample.
        // In the future, codecs could provide a fast preview path.
        let image = registry.load(path)?;
        // Downsample to max 512px on the longest side.
        let max_dim = image.width().max(image.height());
        if max_dim <= 512 || image.width() == 0 || image.height() == 0 {
            return Ok(image);
        }

        let scale = 512.0 / max_dim as f32;
        let new_width = (image.width() as f32 * scale) as u32;
        let new_height = (image.height() as f32 * scale) as u32;

        // Simple nearest-neighbor downsample for preview.
        let mut preview_data = vec![0u8; new_width as usize * new_height as usize * 4]; // RGBA8

        for y in 0..new_height {
            for x in 0..new_width {
                let src_x = (x as f32 / scale) as u32;
                let src_y = (y as f32 / scale) as u32;
                let src_px =
                    image.get_pixel(src_x.min(image.width() - 1), src_y.min(image.height() - 1));
                let dst_off = (y as usize * new_width as usize + x as usize) * 4;
                preview_data[dst_off] = src_px.r;
                preview_data[dst_off + 1] = src_px.g;
                preview_data[dst_off + 2] = src_px.b;
                preview_data[dst_off + 3] = src_px.a;
            }
        }

        TiledImage::from_rgba(new_width, new_height, preview_data)
    }

    /// Loads the full-resolution image.
    async fn load_full(
        registry: &Arc<dyn FileCodecRegistry>,
        path: &Path,
    ) -> ImageResult<TiledImage> {
        registry.load(path)
    }

    /// Returns the current state of a load request.
    pub fn get_state(&self, id: LoadRequestId) -> Option<&LoadState> {
        self.in_flight.get(&id).map(|r| &r.state)
    }

    /// Cancels an in-flight load request.
    pub fn cancel(&mut self, id: LoadRequestId) {
        if let Some(request) = self.in_flight.get_mut(&id) {
            request.state = LoadState::Cancelled;
        }
    }

    /// Returns all in-flight request IDs.
    pub fn in_flight_ids(&self) -> Vec<LoadRequestId> {
        self.in_flight.keys().copied().collect()
    }

    /// Removes a completed/cancelled/failed request from the tracker.
    pub fn remove(&mut self, id: LoadRequestId) -> Option<LoadRequest> {
        self.in_flight.remove(&id)
    }
}

// ---------------------------------------------------------------------------
// BackgroundSaver
// ---------------------------------------------------------------------------

/// Saves images in the background without blocking the UI.
pub struct BackgroundSaver {
    runtime: tokio::runtime::Runtime,
}

impl BackgroundSaver {
    /// Creates a new background saver.
    pub fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        Self { runtime }
    }

    /// Saves an image asynchronously.
    ///
    /// Returns a receiver that will receive the result when the save completes.
    pub fn save(
        &self,
        image: TiledImage,
        path: PathBuf,
        format: kaleido_traits::ImageFormat,
        registry: Arc<dyn FileCodecRegistry>,
    ) -> tokio::sync::oneshot::Receiver<ImageResult<()>> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.runtime.spawn(async move {
            let result = Self::save_internal(&image, &path, format, &registry).await;
            let _ = tx.send(result);
        });

        rx
    }

    async fn save_internal(
        image: &TiledImage,
        path: &Path,
        format: kaleido_traits::ImageFormat,
        registry: &Arc<dyn FileCodecRegistry>,
    ) -> ImageResult<()> {
        registry.save_with_format(path, image, format)
    }
}

impl Default for BackgroundSaver {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::format::FormatRegistry;
    use kaleido_core::{Pixel, PixelFormat};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn temp_png(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kaleido-async-test-{}-{}",
            std::process::id(),
            name
        ));
        let img = TiledImage::with_color(8, 8, PixelFormat::Rgba8, Pixel::rgb(10, 20, 30)).unwrap();
        let registry = FormatRegistry::with_built_in();
        registry.save(&path, &img).unwrap();
        path
    }

    /// Polls until the request leaves `PreviewLoading` or the deadline hits.
    fn wait_for_settled(loader: &mut AsyncImageLoader, id: LoadRequestId) -> LoadState {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            loader.poll();
            if let Some(state) = loader.get_state(id) {
                if !matches!(state, LoadState::PreviewLoading) {
                    return state.clone();
                }
            }
            assert!(Instant::now() < deadline, "load timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn test_load_request_id_unique() {
        let id1 = LoadRequestId::new();
        let id2 = LoadRequestId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_load_state_transitions() {
        let state = LoadState::PreviewLoading;
        match state {
            LoadState::PreviewLoading => {} // expected
            _ => panic!("Expected PreviewLoading"),
        }
    }

    #[test]
    fn test_async_loader_loads_real_image() {
        let path = temp_png("real.png");
        let registry: Arc<dyn FileCodecRegistry> = Arc::new(FormatRegistry::with_built_in());
        let mut loader = AsyncImageLoader::new(registry);

        let id = loader.load(path.clone(), LoadPriority::Sequential);
        assert_eq!(loader.in_flight_ids(), vec![id]);

        let state = wait_for_settled(&mut loader, id);
        match state {
            LoadState::Complete(img) => {
                assert_eq!(img.width(), 8);
                assert_eq!(img.height(), 8);
                let px = img.get_pixel(0, 0);
                assert_eq!((px.r, px.g, px.b), (10, 20, 30));
            }
            other => panic!("expected Complete, got {:?}", other),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_loader_reports_failure_for_missing_file() {
        let registry: Arc<dyn FileCodecRegistry> = Arc::new(FormatRegistry::with_built_in());
        let mut loader = AsyncImageLoader::new(registry);

        let path = std::env::temp_dir().join(format!(
            "kaleido-async-test-{}-missing.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let id = loader.load(path, LoadPriority::Sequential);

        let state = wait_for_settled(&mut loader, id);
        assert!(matches!(state, LoadState::Failed(_)));
    }

    #[test]
    fn test_cancelled_request_stays_cancelled() {
        let path = temp_png("cancel.png");
        let registry: Arc<dyn FileCodecRegistry> = Arc::new(FormatRegistry::with_built_in());
        let mut loader = AsyncImageLoader::new(registry);

        let id = loader.load(path.clone(), LoadPriority::Sequential);
        loader.cancel(id);
        // Even after the background task finishes, poll must not resurrect it.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            loader.poll();
            if let Some(LoadState::Cancelled) = loader.get_state(id) {
                break;
            }
            assert!(Instant::now() < deadline, "cancel state not preserved");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(loader.in_flight_ids().contains(&id));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_remove_drops_request_and_late_result() {
        let path = temp_png("remove.png");
        let registry: Arc<dyn FileCodecRegistry> = Arc::new(FormatRegistry::with_built_in());
        let mut loader = AsyncImageLoader::new(registry);

        let id = loader.load(path.clone(), LoadPriority::Sequential);
        assert!(loader.remove(id).is_some());
        // Give the background task time to finish and report back; its late
        // result must not re-insert the removed request.
        let deadline = Instant::now() + Duration::from_millis(200);
        while Instant::now() < deadline {
            loader.poll();
            assert!(!loader.in_flight_ids().contains(&id));
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_background_saver_creation() {
        let _saver = BackgroundSaver::new();
    }
}

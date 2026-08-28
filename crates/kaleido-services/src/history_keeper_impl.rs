use std::sync::{Arc, RwLock, Weak};
use std::time::SystemTime;

use cordis::{Context, Service};
use kaleido_core::{Image, ImageResult};
use kaleido_traits::{
    Command, HistoryChangedEvent, HistoryEntry, HistoryError, HistoryKeeper, HistoryResult,
    ImageStore, KaleidoEmitter,
};

use tracing::{info, warn};

// ---------------------------------------------------------------------------
// SnapshotCommand — MVP command using before/after image snapshots
// ---------------------------------------------------------------------------

/// A command that stores full-image snapshots for undo/redo.
///
/// This is the MVP implementation. Because `Image` uses `Arc<Vec<u8>>`
/// internally, cloning is cheap (reference-counted, not a full copy).
pub struct SnapshotCommand {
    before: Image,
    after: Image,
    name: String,
    description: String,
    timestamp: SystemTime,
}

impl SnapshotCommand {
    /// Creates a new [`SnapshotCommand`].
    ///
    /// * `before` — the image state *before* the operation.
    /// * `after`  — the image state *after* the operation.
    /// * `name`   — short name for the history panel.
    /// * `description` — longer description.
    pub fn new(
        before: Image,
        after: Image,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            before,
            after,
            name: name.into(),
            description: description.into(),
            timestamp: SystemTime::now(),
        }
    }

    /// Returns the timestamp of this command.
    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

impl Command for SnapshotCommand {
    fn execute(&self, _image: &Image) -> ImageResult<Image> {
        Ok(self.after.clone())
    }

    fn undo(&self, _image: &Image) -> ImageResult<Image> {
        Ok(self.before.clone())
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        self.description.clone()
    }
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

/// Shared state for [`HistoryKeeperImpl`], protected by a single `RwLock`.
struct HistoryState {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_steps: usize,
}

impl HistoryState {
    fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_steps: 50,
        }
    }
}

// ---------------------------------------------------------------------------
// HistoryKeeperImpl
// ---------------------------------------------------------------------------

/// Default implementation of [`HistoryKeeper`].
///
/// Holds a weak reference to [`ImageStore`] to avoid a circular `Arc`
/// dependency, and a [`Context`] clone to emit `history_changed` events
/// through the Cordis event system.
pub struct HistoryKeeperImpl {
    state: Arc<RwLock<HistoryState>>,
    image_store: Weak<dyn ImageStore>,
    ctx: Context,
}

impl HistoryKeeperImpl {
    /// Creates a new [`HistoryKeeperImpl`].
    pub fn new(image_store: Weak<dyn ImageStore>, ctx: Context) -> Self {
        Self {
            state: Arc::new(RwLock::new(HistoryState::new())),
            image_store,
            ctx,
        }
    }

    /// Emits a `history_changed` event through the Cordis event system.
    fn emit_history_changed(&self) {
        let state = match self.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("History state lock poisoned, recovering");
                poisoned.into_inner()
            }
        };

        let undo_count = state.undo_stack.len();
        let redo_count = state.redo_stack.len();
        drop(state);

        self.ctx.emit_history_changed(HistoryChangedEvent {
            undo_count,
            redo_count,
        });
    }
}

impl Default for HistoryKeeperImpl {
    fn default() -> Self {
        panic!("HistoryKeeperImpl requires ImageStore and Context — use `new()` instead");
    }
}

impl Service for HistoryKeeperImpl {
    const NAME: &'static str = "history_keeper";
}

impl HistoryKeeper for HistoryKeeperImpl {
    fn push(&self, command: Box<dyn Command>) -> HistoryResult<()> {
        let mut state = self
            .state
            .write()
            .map_err(|_| HistoryError::StoreUnavailable)?;

        state.undo_stack.push(command);
        state.redo_stack.clear();

        // Enforce max_steps limit — discard oldest entries.
        while state.undo_stack.len() > state.max_steps {
            state.undo_stack.remove(0);
        }

        info!(
            "History: pushed command (undo: {}, redo: {})",
            state.undo_stack.len(),
            state.redo_stack.len()
        );

        drop(state);
        self.emit_history_changed();
        Ok(())
    }

    fn undo(&self) -> HistoryResult<()> {
        // Check stack emptiness BEFORE accessing the store.
        let mut state = self
            .state
            .write()
            .map_err(|_| HistoryError::StoreUnavailable)?;

        let command = state.undo_stack.pop().ok_or(HistoryError::NothingToUndo)?;

        let store = self
            .image_store
            .upgrade()
            .ok_or(HistoryError::StoreUnavailable)?;

        // Compute the previous image state.
        let current = store.get_image()?;
        let restored = command.undo(current.as_ref().unwrap())?;

        // Move command to redo stack.
        state.redo_stack.push(command);

        let undo_count = state.undo_stack.len();
        let redo_count = state.redo_stack.len();
        drop(state);

        // Restore the image in the store.
        store.restore_state(restored)?;

        self.emit_history_changed();
        info!("History: undo (undo: {}, redo: {})", undo_count, redo_count);
        Ok(())
    }

    fn redo(&self) -> HistoryResult<()> {
        // Check stack emptiness BEFORE accessing the store.
        let mut state = self
            .state
            .write()
            .map_err(|_| HistoryError::StoreUnavailable)?;

        let command = state.redo_stack.pop().ok_or(HistoryError::NothingToRedo)?;

        let store = self
            .image_store
            .upgrade()
            .ok_or(HistoryError::StoreUnavailable)?;

        // Compute the redo image state.
        let current = store.get_image()?;
        let applied = command.execute(current.as_ref().unwrap())?;

        // Move command back to undo stack.
        state.undo_stack.push(command);

        let undo_count = state.undo_stack.len();
        let redo_count = state.redo_stack.len();
        drop(state);

        // Apply the image in the store.
        store.restore_state(applied)?;

        self.emit_history_changed();
        info!("History: redo (undo: {}, redo: {})", undo_count, redo_count);
        Ok(())
    }

    fn clear(&self) {
        let mut state = match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("History state lock poisoned in clear()");
                poisoned.into_inner()
            }
        };

        state.undo_stack.clear();
        state.redo_stack.clear();
        drop(state);

        self.emit_history_changed();
        info!("History: cleared");
    }

    fn can_undo(&self) -> bool {
        self.state
            .read()
            .map(|s| !s.undo_stack.is_empty())
            .unwrap_or(false)
    }

    fn can_redo(&self) -> bool {
        self.state
            .read()
            .map(|s| !s.redo_stack.is_empty())
            .unwrap_or(false)
    }

    fn history_list(&self) -> Vec<HistoryEntry> {
        let state = match self.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("History state lock poisoned in history_list()");
                poisoned.into_inner()
            }
        };

        // Build entries from the undo stack only (oldest first).
        // Redo-stack entries are excluded — they represent operations that
        // have been undone and are no longer part of the active history.
        state
            .undo_stack
            .iter()
            .map(|cmd| HistoryEntry {
                name: cmd.name(),
                description: cmd.description(),
                timestamp: SystemTime::now(),
                image_size: (0, 0),
            })
            .collect()
    }

    fn current_index(&self) -> usize {
        self.state.read().map(|s| s.undo_stack.len()).unwrap_or(0)
    }

    fn total_count(&self) -> usize {
        self.state
            .read()
            .map(|s| s.undo_stack.len() + s.redo_stack.len())
            .unwrap_or(0)
    }

    fn set_max_steps(&self, max: usize) {
        let mut state = match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("History state lock poisoned in set_max_steps()");
                poisoned.into_inner()
            }
        };

        state.max_steps = max;

        // Trim if currently over the new limit.
        while state.undo_stack.len() > state.max_steps {
            state.undo_stack.remove(0);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use cordis::{Context, Event as CordisEvent};
    use kaleido_core::{ImageError, Pixel, PixelFormat};
    use kaleido_traits::{HistoryChangedEvent, HISTORY_CHANGED};

    // ─── Test helpers ──

    /// A test image store that holds a single image.
    struct TestImageStore {
        current: Arc<RwLock<Option<Image>>>,
    }

    impl TestImageStore {
        fn new() -> Self {
            Self {
                current: Arc::new(RwLock::new(None)),
            }
        }
    }

    impl ImageStore for TestImageStore {
        fn open(&self, _path: &std::path::Path) -> ImageResult<()> {
            Ok(())
        }

        fn save(&self) -> ImageResult<()> {
            Ok(())
        }

        fn save_as(&self, _path: &std::path::Path) -> ImageResult<()> {
            Ok(())
        }

        fn get_image(&self) -> ImageResult<Option<Image>> {
            Ok(self.current.read().unwrap().clone())
        }

        fn get_dimensions(&self) -> Option<(u32, u32)> {
            self.current
                .read()
                .unwrap()
                .as_ref()
                .map(|img| (img.width(), img.height()))
        }

        fn get_format(&self) -> Option<PixelFormat> {
            self.current
                .read()
                .unwrap()
                .as_ref()
                .map(|img| img.format())
        }

        fn get_path(&self) -> Option<std::path::PathBuf> {
            None
        }

        fn get_metadata(&self) -> Option<kaleido_core::ImageMetadata> {
            self.current
                .read()
                .unwrap()
                .as_ref()
                .map(|img| img.metadata().clone())
        }

        fn has_image(&self) -> bool {
            self.current.read().unwrap().is_some()
        }

        fn apply_mutation(
            &self,
            mutator: Box<dyn FnOnce(&mut Image) -> ImageResult<()>>,
        ) -> ImageResult<()> {
            let mut current = self.current.write().unwrap();
            let img = current.as_mut().ok_or(ImageError::EmptyImage)?;
            mutator(img)?;
            Ok(())
        }

        fn set_image(&self, image: Image) -> ImageResult<()> {
            let mut current = self.current.write().unwrap();
            *current = Some(image);
            Ok(())
        }

        fn snapshot(&self) -> ImageResult<Option<Image>> {
            self.get_image()
        }

        fn restore_state(&self, image: Image) -> ImageResult<()> {
            let mut current = self.current.write().unwrap();
            *current = Some(image);
            Ok(())
        }

        fn clear(&self) {
            let mut current = self.current.write().unwrap();
            *current = None;
        }
    }

    /// Creates a HistoryKeeperImpl with a real Cordis context wired to a
    /// `history_changed` listener that records emitted events.
    fn create_keeper() -> (HistoryKeeperImpl, Arc<dyn ImageStore>, Arc<RwLock<Vec<CordisEvent>>>) {
        let store = Arc::new(TestImageStore::new());
        let ctx = Context::new();
        let events: Arc<RwLock<Vec<CordisEvent>>> = Arc::new(RwLock::new(Vec::new()));

        let recorded = events.clone();
        let _ = ctx.on(HISTORY_CHANGED, move |event| {
            recorded.write().unwrap().push(event);
            Ok(None)
        });

        let store_weak = Arc::downgrade(&store);
        let keeper = HistoryKeeperImpl::new(store_weak, ctx);
        (keeper, store, events)
    }

    // ─── Tests ──

    #[test]
    fn test_push_and_undo() {
        let (keeper, store, _,) = create_keeper();

        // Set initial image.
        let img1 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        // Create a modified image.
        let img2 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();

        // Push a command.
        let cmd = SnapshotCommand::new(
            img1.clone(),
            img2.clone(),
            "Brightness",
            "Adjust brightness",
        );
        keeper.push(Box::new(cmd)).unwrap();

        assert!(keeper.can_undo());
        assert!(!keeper.can_redo());
        assert_eq!(keeper.current_index(), 1);

        // Undo.
        keeper.undo().unwrap();

        // Verify image was restored.
        let restored = store.get_image().unwrap().unwrap();
        let pixel = restored.get_pixel(0, 0).unwrap();
        assert_eq!(pixel.r, 255);
        assert_eq!(pixel.g, 0);

        assert!(!keeper.can_undo());
        assert!(keeper.can_redo());
    }

    #[test]
    fn test_undo_and_redo() {
        let (keeper, store, _,) = create_keeper();

        let img1 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();

        let cmd = SnapshotCommand::new(
            img1.clone(),
            img2.clone(),
            "Brightness",
            "Adjust brightness",
        );
        keeper.push(Box::new(cmd)).unwrap();

        // Undo.
        keeper.undo().unwrap();

        // Redo.
        keeper.redo().unwrap();

        // Verify image is back to the "after" state.
        let redone = store.get_image().unwrap().unwrap();
        let pixel = redone.get_pixel(0, 0).unwrap();
        assert_eq!(pixel.r, 0);
        assert_eq!(pixel.g, 255);

        assert!(keeper.can_undo());
        assert!(!keeper.can_redo());
    }

    #[test]
    fn test_cannot_undo_on_empty() {
        let (keeper, _, _,) = create_keeper();

        let result = keeper.undo();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), HistoryError::NothingToUndo);
    }

    #[test]
    fn test_cannot_redo_on_empty() {
        let (keeper, _, _,) = create_keeper();

        let result = keeper.redo();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), HistoryError::NothingToRedo);
    }

    #[test]
    fn test_max_steps_limit() {
        let (keeper, store, _,) = create_keeper();

        let img1 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        keeper.set_max_steps(3);

        // Push 5 commands.
        for i in 0..5 {
            let after =
                Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(i * 50, 0, 0)).unwrap();
            let before = store.get_image().unwrap().unwrap();
            let cmd = SnapshotCommand::new(before, after, &format!("Op {}", i), "Test op");
            keeper.push(Box::new(cmd)).unwrap();
        }

        // Should only have 3 commands (max_steps).
        assert_eq!(keeper.current_index(), 3);
        assert_eq!(keeper.total_count(), 3);
    }

    #[test]
    fn test_clear_empties_history() {
        let (keeper, store, _,) = create_keeper();

        let img1 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();
        let cmd = SnapshotCommand::new(img1.clone(), img2, "Test", "Test op");
        keeper.push(Box::new(cmd)).unwrap();

        assert!(keeper.can_undo());

        keeper.clear();

        assert!(!keeper.can_undo());
        assert!(!keeper.can_redo());
        assert_eq!(keeper.total_count(), 0);
    }

    #[test]
    fn test_new_command_clears_redo_stack() {
        let (keeper, store, _,) = create_keeper();

        let img1 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();
        let cmd1 = SnapshotCommand::new(img1.clone(), img2.clone(), "Op1", "First op");
        keeper.push(Box::new(cmd1)).unwrap();

        // Undo to create a redo entry.
        keeper.undo().unwrap();
        assert!(keeper.can_redo());

        // Push a new command — should clear redo stack.
        let img3 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 255)).unwrap();
        let cmd2 = SnapshotCommand::new(img1.clone(), img3, "Op2", "Second op");
        keeper.push(Box::new(cmd2)).unwrap();

        assert!(!keeper.can_redo());
        assert_eq!(keeper.current_index(), 1);
    }

    #[test]
    fn test_history_list_order() {
        let (keeper, store, _,) = create_keeper();

        let img1 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        // Push 3 commands.
        for i in 1..=3 {
            let after =
                Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(i * 50, 0, 0)).unwrap();
            let before = store.get_image().unwrap().unwrap();
            let cmd = SnapshotCommand::new(before, after, &format!("Op {}", i), "Test");
            keeper.push(Box::new(cmd)).unwrap();
        }

        let list = keeper.history_list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "Op 1");
        assert_eq!(list[1].name, "Op 2");
        assert_eq!(list[2].name, "Op 3");
    }

    #[test]
    fn test_history_changed_event_emitted() {
        let (keeper, store, events,) = create_keeper();

        let img1 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();
        let cmd = SnapshotCommand::new(img1.clone(), img2, "Test", "Test op");
        keeper.push(Box::new(cmd)).unwrap();

        // Verify the history_changed event was emitted synchronously.
        let events = events.read().unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.name(), HISTORY_CHANGED);
        let payload = event
            .arg::<HistoryChangedEvent>(0)
            .unwrap()
            .expect("history_changed payload missing");
        assert_eq!(payload.undo_count, 1);
        assert_eq!(payload.redo_count, 0);
    }

    #[test]
    fn test_snapshot_command_execute_and_undo() {
        let before = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        let after = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();

        let cmd = SnapshotCommand::new(before.clone(), after.clone(), "Test", "Test op");

        // execute() returns the "after" image.
        let result = cmd.execute(&before).unwrap();
        assert_eq!(result.get_pixel(0, 0).unwrap().g, 255);

        // undo() returns the "before" image.
        let result = cmd.undo(&after).unwrap();
        assert_eq!(result.get_pixel(0, 0).unwrap().r, 255);
    }
}

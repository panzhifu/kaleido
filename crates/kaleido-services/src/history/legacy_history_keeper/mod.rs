use std::sync::{Arc, RwLock, Weak};
use std::time::SystemTime;

use cordis::{Context, Service};
use kaleido_core::ImageError;
use kaleido_traits::{
    Command, HistoryChangedEvent, HistoryEntry, HistoryError, HistoryKeeper, HistoryResult,
    ImageStore, KaleidoEmitter,
};

use crate::services::history::tile_history::TileSnapshotCommand;

use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

/// Shared state for [`HistoryKeeperImpl`], protected by a single `RwLock`.
struct HistoryState {
    undo_stack: Vec<TileSnapshotCommand>,
    redo_stack: Vec<TileSnapshotCommand>,
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
/// Uses dirty-tile snapshots ([`TileSnapshotCommand`]) for memory-efficient
/// undo/redo.  Only modified tiles are stored per operation, so memory
/// usage is proportional to the changed region rather than the full image.
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

    /// Pushes a command back onto the undo stack — used to roll back after
    /// a failed apply / restore in [`HistoryKeeper::undo`] so the command
    /// is never lost.
    fn push_undo(&self, command: TileSnapshotCommand) {
        let mut state = match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("History state lock poisoned in rollback, recovering");
                poisoned.into_inner()
            }
        };
        state.undo_stack.push(command);
    }

    /// Pushes a command back onto the redo stack — the `redo` counterpart
    /// of [`Self::push_undo`].
    fn push_redo(&self, command: TileSnapshotCommand) {
        let mut state = match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("History state lock poisoned in rollback, recovering");
                poisoned.into_inner()
            }
        };
        state.redo_stack.push(command);
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
        // Convert Box<dyn Command> to Box<dyn Any> for downcasting.
        let any = command.into_any();
        let tile_cmd = match any.downcast::<TileSnapshotCommand>() {
            Ok(cmd) => *cmd,
            Err(_) => {
                warn!("HistoryKeeperImpl received a non-TileSnapshotCommand; skipping");
                return Ok(());
            }
        };

        let mut state = self
            .state
            .write()
            .map_err(|_| HistoryError::StoreUnavailable)?;

        state.undo_stack.push(tile_cmd);
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
        // Pop the command under the lock, then drop the guard before
        // touching the store: `restore_state`/`get_image` may emit events,
        // and holding the (non-reentrant) write lock across those calls
        // would deadlock any listener that reads the stacks back.
        let command = {
            let mut state = self
                .state
                .write()
                .map_err(|_| HistoryError::StoreUnavailable)?;
            state.undo_stack.pop().ok_or(HistoryError::NothingToUndo)?
        };

        let store = self
            .image_store
            .upgrade()
            .ok_or(HistoryError::StoreUnavailable)?;

        // Get the current image and apply the undo.
        let mut current = store.get_image()?.ok_or(ImageError::EmptyImage)?;

        // A failed apply / restore pushes the command back onto the undo
        // stack so no history is lost.
        if let Err(err) = command.apply_before(&mut current) {
            self.push_undo(command);
            return Err(err.into());
        }
        if let Err(err) = store.restore_state(current) {
            self.push_undo(command);
            return Err(err.into());
        }

        let (undo_count, redo_count) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| HistoryError::StoreUnavailable)?;
            // Move command to redo stack.
            state.redo_stack.push(command);
            (state.undo_stack.len(), state.redo_stack.len())
        };

        self.emit_history_changed();
        info!("History: undo (undo: {}, redo: {})", undo_count, redo_count);
        Ok(())
    }

    fn redo(&self) -> HistoryResult<()> {
        // See `undo` — the state guard is dropped before store calls to
        // avoid deadlocking listeners on the non-reentrant RwLock.
        let command = {
            let mut state = self
                .state
                .write()
                .map_err(|_| HistoryError::StoreUnavailable)?;
            state.redo_stack.pop().ok_or(HistoryError::NothingToRedo)?
        };

        let store = self
            .image_store
            .upgrade()
            .ok_or(HistoryError::StoreUnavailable)?;

        // Get the current image and apply the redo.
        let mut current = store.get_image()?.ok_or(ImageError::EmptyImage)?;

        if let Err(err) = command.apply_after(&mut current) {
            self.push_redo(command);
            return Err(err.into());
        }
        if let Err(err) = store.restore_state(current) {
            self.push_redo(command);
            return Err(err.into());
        }

        let (undo_count, redo_count) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| HistoryError::StoreUnavailable)?;
            // Move command back to undo stack.
            state.undo_stack.push(command);
            (state.undo_stack.len(), state.redo_stack.len())
        };

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
    use crate::services::history::tile_history::TileSnapshotCommand;
    use kaleido_core::{Pixel, PixelFormat, TiledImage};

    // ─── Test helpers ──

    /// A test image store that holds a single image.
    struct TestImageStore {
        current: Arc<RwLock<Option<TiledImage>>>,
    }

    impl TestImageStore {
        fn new() -> Self {
            Self {
                current: Arc::new(RwLock::new(None)),
            }
        }
    }

    impl ImageStore for TestImageStore {
        fn open(&self, _path: &std::path::Path) -> kaleido_core::ImageResult<()> {
            Ok(())
        }
        fn save(&self) -> kaleido_core::ImageResult<()> {
            Ok(())
        }
        fn save_as(&self, _path: &std::path::Path) -> kaleido_core::ImageResult<()> {
            Ok(())
        }
        fn get_image(&self) -> kaleido_core::ImageResult<Option<TiledImage>> {
            Ok(self.current.read().unwrap().clone())
        }
        fn get_dimensions(&self) -> Option<(u32, u32)> {
            self.current.read().unwrap().as_ref().map(|img| (img.width(), img.height()))
        }
        fn get_format(&self) -> Option<PixelFormat> {
            self.current.read().unwrap().as_ref().map(|img| img.format())
        }
        fn get_path(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn get_metadata(&self) -> Option<kaleido_core::ImageMetadata> {
            self.current.read().unwrap().as_ref().map(|img| img.metadata().clone())
        }
        fn has_image(&self) -> bool {
            self.current.read().unwrap().is_some()
        }
        fn apply_mutation(
            &self,
            mutator: Box<dyn FnOnce(&mut TiledImage) -> kaleido_core::ImageResult<()>>,
        ) -> kaleido_core::ImageResult<()> {
            let mut current = self.current.write().unwrap();
            let img = current.as_mut().ok_or(kaleido_core::ImageError::EmptyImage)?;
            mutator(img)?;
            Ok(())
        }
        fn set_image(&self, new_image: TiledImage) -> kaleido_core::ImageResult<()> {
            let mut current = self.current.write().unwrap();
            *current = Some(new_image);
            Ok(())
        }
        fn snapshot(&self) -> kaleido_core::ImageResult<Option<TiledImage>> {
            self.get_image()
        }
        fn restore_state(&self, image: TiledImage) -> kaleido_core::ImageResult<()> {
            let mut current = self.current.write().unwrap();
            *current = Some(image);
            Ok(())
        }
        fn clear(&self) {
            let mut current = self.current.write().unwrap();
            *current = None;
        }
    }

    fn create_store() -> (Arc<TestImageStore>, Context) {
        let ctx = Context::new();
        let store = Arc::new(TestImageStore::new());
        (store, ctx)
    }

    fn setup_keeper() -> (Arc<TestImageStore>, HistoryKeeperImpl) {
        let (store, ctx) = create_store();
        let keeper = HistoryKeeperImpl::new(
            Arc::downgrade(&(store.clone() as Arc<dyn ImageStore>)),
            ctx,
        );
        (store, keeper)
    }

    #[test]
    fn test_push_and_undo() {
        let (store, keeper) = setup_keeper();
        let img1 =
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 =
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();

        // Create a tile-based command from the diff.
        let cmd = TileSnapshotCommand::from_diff(&img1, &img2, "Test", "Test op");
        keeper.push(Box::new(cmd)).unwrap();

        assert!(keeper.can_undo());
        assert_eq!(keeper.current_index(), 1);

        // Undo.
        keeper.undo().unwrap();
        assert!(!keeper.can_undo());
        assert!(keeper.can_redo());

        // Verify the image was restored.
        let restored = store.get_image().unwrap().unwrap();
        assert_eq!(restored.get_pixel(0, 0), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_redo() {
        let (store, keeper) = setup_keeper();
        let img1 =
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 =
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();

        let cmd = TileSnapshotCommand::from_diff(&img1, &img2, "Test", "Test op");
        keeper.push(Box::new(cmd)).unwrap();

        // Undo then redo.
        keeper.undo().unwrap();
        keeper.redo().unwrap();

        // Verify the image is back to the "after" state.
        let redone = store.get_image().unwrap().unwrap();
        assert_eq!(redone.get_pixel(0, 0), Pixel::rgb(0, 255, 0));
    }

    #[test]
    fn test_max_steps() {
        let (store, keeper) = setup_keeper();
        let img1 =
            TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        // Push 5 commands with max_steps=3.
        keeper.set_max_steps(3);
        for i in 0..5 {
            let after =
                TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(i * 50, 0, 0)).unwrap();
            let before = store.get_image().unwrap().unwrap();
            let cmd = TileSnapshotCommand::from_diff(&before, &after, &format!("Op {}", i), "Test");
            keeper.push(Box::new(cmd)).unwrap();
            // Update the store to the new image for the next iteration.
            store.set_image(after).unwrap();
        }

        assert_eq!(keeper.current_index(), 3);
    }

    #[test]
    fn test_clear() {
        let (store, keeper) = setup_keeper();
        let img1 =
            TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 =
            TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 255, 255)).unwrap();
        let cmd = TileSnapshotCommand::from_diff(&img1, &img2, "Test", "Test op");
        keeper.push(Box::new(cmd)).unwrap();

        assert!(keeper.can_undo());
        keeper.clear();
        assert!(!keeper.can_undo());
        assert!(!keeper.can_redo());
    }

    #[test]
    fn test_history_list() {
        let (store, keeper) = setup_keeper();
        let img1 =
            TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 =
            TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 255, 255)).unwrap();
        let cmd = TileSnapshotCommand::from_diff(&img1, &img2, "Brightness", "Adjust brightness");
        keeper.push(Box::new(cmd)).unwrap();

        let list = keeper.history_list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Brightness");
    }

    #[test]
    fn test_nothing_to_undo() {
        let (_store, keeper) = setup_keeper();
        assert!(keeper.undo().is_err());
    }

    #[test]
    fn test_nothing_to_redo() {
        let (_store, keeper) = setup_keeper();
        assert!(keeper.redo().is_err());
    }
}

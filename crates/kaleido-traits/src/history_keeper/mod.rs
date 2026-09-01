use std::time::SystemTime;

use kaleido_core::{ImageError, ImageResult, TiledImage};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Command trait — the unit of undoable work
// ---------------------------------------------------------------------------

/// A reversible image operation.
///
/// Each command stores enough information to apply (`execute`) and reverse
/// (`undo`) an image mutation.  The standard implementation is
/// [`crate::tile_history::TileSnapshotCommand`], which stores dirty-tile
/// diffs for memory-efficient undo/redo.
pub trait Command: Send + Sync + 'static {
    /// Produces the image **after** this command is applied.
    fn execute(&self, image: &TiledImage) -> ImageResult<TiledImage>;

    /// Produces the image **before** this command was applied.
    fn undo(&self, image: &TiledImage) -> ImageResult<TiledImage>;

    /// Short human-readable name for the history panel.
    fn name(&self) -> String;

    /// Longer description shown in tooltips, etc.
    fn description(&self) -> String;

    /// Returns `self` as `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Consumes a `Box<dyn Command>` and returns `Box<dyn Any>` for owned downcasting.
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any>;
}

// ---------------------------------------------------------------------------
// HistoryEntry — a record displayed in the UI
// ---------------------------------------------------------------------------

/// An entry in the undo/redo history list.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Command name (e.g. "Brightness", "Crop").
    pub name: String,
    /// Longer description of the operation.
    pub description: String,
    /// When the command was executed.
    pub timestamp: SystemTime,
    /// Image dimensions at the time of the operation.
    pub image_size: (u32, u32),
}

// ---------------------------------------------------------------------------
// HistoryError
// ---------------------------------------------------------------------------

/// Errors that can occur inside [`HistoryKeeper`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HistoryError {
    /// The undo stack is empty.
    #[error("Nothing to undo")]
    NothingToUndo,

    /// The redo stack is empty.
    #[error("Nothing to redo")]
    NothingToRedo,

    /// The referenced `ImageStore` has been dropped.
    #[error("ImageStore is no longer available")]
    StoreUnavailable,

    /// An image-level error propagated from a command.
    #[error("Image error: {0}")]
    ImageError(#[from] ImageError),
}

/// Convenient alias for `Result<T, HistoryError>`.
pub type HistoryResult<T> = std::result::Result<T, HistoryError>;

// ---------------------------------------------------------------------------
// HistoryKeeper trait
// ---------------------------------------------------------------------------

/// Manages undo/redo history for image operations.
///
/// Every image mutation is recorded as a [`Command`] so the user can step
/// backward (`undo`) and forward (`redo`) through their editing history.
///
/// # Design Principles
///
/// - **Command Pattern**: Each history entry is a self-contained `Command`.
/// - **Bounded**: A configurable `max_steps` caps memory usage.
/// - **Event-Driven**: Subscribes to `ImageChanged` to auto-record.
/// - **Weak References**: Holds `Weak<dyn ImageStore>` to avoid cycles.
pub trait HistoryKeeper: Send + Sync + 'static {
    // ─── Core operations ───

    /// Records a completed operation.
    ///
    /// The command is pushed onto the undo stack and the redo stack is
    /// cleared. A `history_changed` event is emitted afterwards.
    fn push(&self, command: Box<dyn Command>) -> HistoryResult<()>;

    /// Undoes the most recent operation, restoring the previous image state.
    ///
    /// The command is moved from the undo stack to the redo stack.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::NothingToUndo`] if the undo stack is empty.
    fn undo(&self) -> HistoryResult<()>;

    /// Redoes the most recently undone operation.
    ///
    /// The command is moved from the redo stack back to the undo stack.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::NothingToRedo`] if the redo stack is empty.
    fn redo(&self) -> HistoryResult<()>;

    /// Clears all history (both undo and redo stacks).
    ///
    /// Called when a new image is loaded or the document is reset.
    /// Emits `history_changed`.
    fn clear(&self);

    // ─── State queries ───

    /// Returns `true` if there is at least one command to undo.
    fn can_undo(&self) -> bool;

    /// Returns `true` if there is at least one command to redo.
    fn can_redo(&self) -> bool;

    /// Returns the full history list (undo + redo), oldest first.
    ///
    /// Used by the UI history panel.
    fn history_list(&self) -> Vec<HistoryEntry>;

    /// Returns the current position within the undo stack (0 = empty).
    fn current_index(&self) -> usize;

    /// Returns the total number of commands across both stacks.
    fn total_count(&self) -> usize;

    /// Sets the maximum number of undo steps (default 50).
    ///
    /// When the limit is exceeded, the oldest command is discarded.
    fn set_max_steps(&self, max: usize);
}

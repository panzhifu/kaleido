//! The **history manager** — undo / redo / history navigation.
//!
//! Manages the undo/redo stacks for document mutations. Every mutation
//! pushed onto the undo stack can be undone and redone.

use serde::{Deserialize, Serialize};

use super::ServiceResult;

/// A history entry — a snapshot of the document at a point in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Stable id (can be referenced for command replay).
    pub id: u64,
    /// Human-readable label ("Brush stroke", "Move node", …).
    pub label: String,
    /// Unix seconds since epoch.
    pub timestamp: i64,
}

/// The history management service.
pub trait HistoryService: Send + Sync + 'static {
    // ── Undo / Redo ─────────────────────────────────────────────────────

    /// Undoes the last mutation.
    ///
    /// # Errors
    ///
    /// Returns `Err` when there is nothing to undo.
    fn undo(&self) -> ServiceResult<()>;

    /// Redoes the last undone mutation.
    ///
    /// # Errors
    ///
    /// Returns `Err` when there is nothing to redo.
    fn redo(&self) -> ServiceResult<()>;

    // ── Queries ─────────────────────────────────────────────────────────

    /// Whether an undo step is available.
    fn can_undo(&self) -> bool;

    /// Whether a redo step is available.
    fn can_redo(&self) -> bool;

    /// Number of undo steps available.
    fn undo_depth(&self) -> usize;

    /// Number of redo steps available.
    fn redo_depth(&self) -> usize;

    /// Label of the most recent mutation on the undo stack (for UI hints),
    /// if any. `None` while the undo stack is empty.
    fn last_label(&self) -> Option<String>;

    // ── Management ──────────────────────────────────────────────────────

    /// Clears the entire undo / redo history.
    fn clear(&self) -> ServiceResult<()>;

    /// Returns all undo entries (most recent first).
    fn undo_entries(&self) -> Vec<HistoryEntry>;

    /// Returns all redo entries (most recent first).
    fn redo_entries(&self) -> Vec<HistoryEntry>;
}

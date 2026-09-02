//! The **data manager** — document lifecycle only.
//!
//! Owns the current [`Document`] and manages its lifecycle:
//! create / open / save / close / query.
//!
//! All content editing (layers, pixels, history) is done by other services
//! through the [`document()`] accessor.

pub use crate::codec::resolve_format_registry;
pub use crate::service_error::{ServiceError, ServiceResult};

use std::path::{Path, PathBuf};

use kaleido_core::{Document, DocumentId, ImageSize};

/// The data management service — document lifecycle.
pub trait DataService: Send + Sync + 'static {
    // ── Internal (History only) ─────────────────────────────────────────

    /// Restores a snapshot as the current document (used by HistoryService).
    /// Not for general use — all mutations should go through the managers.
    fn restore(&self, snapshot: Document);

    /// Restores from a history snapshot (full or dirty-tile).
    fn restore_snapshot(&self, snapshot: &crate::history::Snapshot) {
        // Default implementation: delegate to restore() for full snapshots.
        match snapshot {
            crate::history::Snapshot::Full(doc) => self.restore(doc.clone()),
            crate::history::Snapshot::DirtyTile(_) => {
                // Dirty-tile snapshots require tile-level access.
                // Override this method for optimized dirty-tile restoration.
            }
        }
    }
    // ── Lifecycle ────────────────────────────────────────────────────────

    /// Creates a new blank document and makes it current.
    fn new_document(&self, name: &str, width: u32, height: u32) -> ServiceResult<DocumentId>;

    /// Opens a document from disk.
    ///
    /// `.kld` files are deserialized as [`Document`]s; any other extension
    /// is decoded as a bitmap (via the [`FileCodecRegistry`](crate::codec::FileCodecRegistry))
    /// and wrapped in a document with one pixel layer.
    ///
    /// On failure the previously open document (if any) is left untouched.
    fn open(&self, path: &Path) -> ServiceResult<()>;

    /// Saves the current document to its file path.
    ///
    /// Errors when no file path is set yet — use [`Self::save_as`] first.
    fn save(&self) -> ServiceResult<()>;

    /// Saves the current document to a specific path.
    ///
    /// `.kld` targets serialize the full document; other extensions are
    /// written as a flattened bitmap. The file path is only updated after a
    /// successful write.
    fn save_as(&self, path: &Path) -> ServiceResult<()>;

    /// Renders the current document to a flat image for bitmap export.
    ///
    /// Composites the scene graph bottom-up into a single RGBA bitmap.
    /// Used by [`save_as`](Self::save_as) when exporting to non-`.kld`
    /// formats.  Returns an error when no document is open.
    fn render_for_export(&self) -> ServiceResult<kaleido_core::TiledImage>;


    /// Closes the current document (no-op when none is open).
    fn close(&self) -> ServiceResult<()>;

    // ── Reads ────────────────────────────────────────────────────────────

    /// A clone of the current document (cheap — tiles are `Arc`-shared).
    ///
    /// Other services obtain the document through this accessor, modify it
    /// directly, and persist changes through their own mechanisms.
    fn document(&self) -> ServiceResult<Option<Document>>;

    /// Whether a document is currently open.
    fn has_document(&self) -> bool;

    /// The current file path, if set.
    fn path(&self) -> Option<PathBuf>;

    /// The current canvas size, if a document is open.
    fn size(&self) -> Option<ImageSize>;
}

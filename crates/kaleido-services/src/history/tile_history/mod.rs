//! Dirty-tile history for memory-efficient undo/redo.
//!
//! [`TileSnapshotCommand`] stores only the tiles that were modified by an
//! operation, not the entire image.  This makes undo/redo memory usage
//! proportional to the modified region rather than the full image size.

use std::collections::HashSet;

use kaleido_core::{ImageError, ImageResult, TileCoord, TiledImage};
use kaleido_traits::Command;

// ---------------------------------------------------------------------------
// TileSnapshot
// ---------------------------------------------------------------------------

/// Stores the before/after state of a single modified tile.
#[derive(Clone)]
pub struct TileSnapshot {
    /// Tile coordinate.
    pub coord: TileCoord,
    /// Tile data before the operation.
    pub before: Vec<u8>,
    /// Tile data after the operation.
    pub after: Vec<u8>,
}

impl TileSnapshot {
    /// Creates a new tile snapshot.
    pub fn new(coord: TileCoord, before: Vec<u8>, after: Vec<u8>) -> Self {
        Self {
            coord,
            before,
            after,
        }
    }
}

// ---------------------------------------------------------------------------
// TileSnapshotCommand
// ---------------------------------------------------------------------------

/// A command that stores dirty-tile snapshots for undo/redo.
///
/// Instead of storing full before/after images (which is wasteful for
/// large images with small modifications), this command stores only the
/// tiles that were actually modified.
///
/// # Absent-tile convention
///
/// An empty (`Vec::is_empty`) buffer in a snapshot means the tile was
/// **absent** on that side of the operation ([`from_diff`] records
/// `unwrap_or_default()` for missing tiles). [`apply_before`]/[`apply_after`]
/// skip empty snapshots rather than writing a zero-length buffer into a
/// fixed-size tile.
#[derive(Clone)]
pub struct TileSnapshotCommand {
    /// Modified tiles (before/after data).
    snapshots: Vec<TileSnapshot>,
    /// Short name for the history panel.
    name: String,
    /// Longer description.
    description: String,
}

impl TileSnapshotCommand {
    /// Creates a new [`TileSnapshotCommand`].
    ///
    /// * `snapshots` — the modified tiles with before/after data.
    /// * `name` — short name for the history panel.
    /// * `description` — longer description.
    pub fn new(
        snapshots: Vec<TileSnapshot>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            snapshots,
            name: name.into(),
            description: description.into(),
        }
    }

    /// Creates a [`TileSnapshotCommand`] by diffing two images.
    ///
    /// Compares `before` and `after` tile-by-tile and stores only the
    /// tiles whose data changed.  This is the primary way to record an
    /// operation for undo/redo.  A tile absent on one side is recorded
    /// with an empty buffer on that side (see the type-level docs).
    pub fn from_diff(
        before: &TiledImage,
        after: &TiledImage,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let mut snapshots = Vec::new();

        // Collect all tile coordinates present in either image, deduplicated.
        let mut seen = HashSet::new();
        let mut all_coords = Vec::new();
        for coord in before.tile_coords().chain(after.tile_coords()) {
            if seen.insert(coord) {
                all_coords.push(coord);
            }
        }

        for coord in all_coords {
            let before_data = before
                .get_tile(coord.col, coord.row)
                .map(|t| t.data().to_vec());
            let after_data = after
                .get_tile(coord.col, coord.row)
                .map(|t| t.data().to_vec());

            // Only store if the tile changed.
            if before_data != after_data {
                snapshots.push(TileSnapshot::new(
                    coord,
                    before_data.unwrap_or_default(),
                    after_data.unwrap_or_default(),
                ));
            }
        }

        Self::new(snapshots, name, description)
    }

    /// Captures the tiles that will be modified by an operation.
    ///
    /// This should be called BEFORE applying the mutation.  It reads the
    /// current tile data for the given coordinates and stores it as the
    /// "before" state.
    pub fn capture_before(
        image: &TiledImage,
        coords: &[TileCoord],
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let snapshots: Vec<TileSnapshot> = coords
            .iter()
            .filter_map(|coord| {
                image
                    .get_tile(coord.col, coord.row)
                    .map(|tile| TileSnapshot::new(*coord, tile.data().to_vec(), tile.data().to_vec()))
            })
            .collect();

        Self::new(snapshots, name, description)
    }

    /// Updates the "after" state of all snapshots.
    ///
    /// This should be called AFTER applying the mutation.  It reads the
    /// current tile data and stores it as the "after" state.
    pub fn capture_after(&mut self, image: &TiledImage) {
        for snapshot in &mut self.snapshots {
            if let Some(tile) = image.get_tile(snapshot.coord.col, snapshot.coord.row) {
                snapshot.after = tile.data().to_vec();
            }
        }
    }

    /// Returns the number of tiles in this command.
    pub fn tile_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns the total payload bytes stored (sum of before + after tile
    /// buffers). This excludes per-snapshot structural overhead (`Vec`
    /// capacities, the snapshot structs themselves, and the name /
    /// description strings) — use [`approx_bytes`](Self::approx_bytes) for a
    /// fuller memory estimate.
    pub fn total_bytes(&self) -> usize {
        self.snapshots
            .iter()
            .map(|s| s.before.len() + s.after.len())
            .sum()
    }

    /// Rough estimate of retained memory: the command struct (its stack
    /// slot), the snapshot array (structs are stored inline in the `Vec`),
    /// the name/description buffers, and every snapshot payload buffer.
    /// Allocator overhead is not counted.
    pub fn approx_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.snapshots.capacity() * std::mem::size_of::<TileSnapshot>()
            + self.name.capacity()
            + self.description.capacity()
            + self
                .snapshots
                .iter()
                .map(|s| s.before.capacity() + s.after.capacity())
                .sum::<usize>()
    }
}

impl Command for TileSnapshotCommand {
    fn execute(&self, image: &TiledImage) -> ImageResult<TiledImage> {
        // TileSnapshotCommand is a marker; the actual execution is
        // handled by the caller via apply_after.
        Ok(image.clone())
    }

    fn undo(&self, image: &TiledImage) -> ImageResult<TiledImage> {
        Ok(image.clone())
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        self.description.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

impl TileSnapshotCommand {
    /// Applies the "after" state to a TiledImage (for redo).
    ///
    /// Empty snapshots are skipped: an empty buffer records a tile that was
    /// absent on the "after" side, and writing zero bytes into a fixed-size
    /// tile buffer would corrupt it. When the recorded buffer does not match
    /// the current tile's size (e.g. the image was rebuilt with a different
    /// pixel format since the snapshot), the restore is rejected with an
    /// error instead of silently leaving the image inconsistent.
    pub fn apply_after(&self, image: &mut TiledImage) -> ImageResult<()> {
        for snapshot in &self.snapshots {
            if snapshot.after.is_empty() {
                continue;
            }
            let tile = image.get_or_create_tile(snapshot.coord.col, snapshot.coord.row);
            let data = tile.data_mut();
            if data.len() != snapshot.after.len() {
                return Err(snapshot_length_error(snapshot.coord, "after", data.len(), snapshot.after.len()));
            }
            data.copy_from_slice(&snapshot.after);
        }
        Ok(())
    }

    /// Applies the "before" state to a TiledImage (for undo).
    ///
    /// Empty snapshots are skipped: an empty buffer records a tile that did
    /// not exist before the operation, and undo cannot remove tiles (known
    /// limitation). When the recorded buffer does not match the current
    /// tile's size, the restore is rejected with an error (see
    /// [`apply_after`](Self::apply_after)).
    pub fn apply_before(&self, image: &mut TiledImage) -> ImageResult<()> {
        for snapshot in &self.snapshots {
            if snapshot.before.is_empty() {
                continue;
            }
            let tile = image.get_or_create_tile(snapshot.coord.col, snapshot.coord.row);
            let data = tile.data_mut();
            if data.len() != snapshot.before.len() {
                return Err(snapshot_length_error(snapshot.coord, "before", data.len(), snapshot.before.len()));
            }
            data.copy_from_slice(&snapshot.before);
        }
        Ok(())
    }
}

/// Builds the error reported when a snapshot buffer cannot be written back
/// because the current tile has a different size.
fn snapshot_length_error(
    coord: TileCoord,
    side: &str,
    current_len: usize,
    snapshot_len: usize,
) -> ImageError {
    ImageError::OperationFailed {
        reason: format!(
            "cannot restore {side} state of tile {coord:?}: current tile length {current_len} != snapshot length {snapshot_len} (image format or geometry changed since the snapshot)",
        ),
    }
}

// ---------------------------------------------------------------------------
// TileHistoryKeeper
// ---------------------------------------------------------------------------

/// History keeper that uses dirty-tile snapshots.
///
/// It stores only the tiles that were modified, making it much more
/// memory-efficient for large images with localized edits.
pub struct TileHistoryKeeper {
    undo_stack: Vec<TileSnapshotCommand>,
    redo_stack: Vec<TileSnapshotCommand>,
    max_steps: usize,
}

impl TileHistoryKeeper {
    /// Creates a new [`TileHistoryKeeper`].
    pub fn new(max_steps: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_steps,
        }
    }

    /// Pushes a command onto the undo stack.
    pub fn push(&mut self, command: TileSnapshotCommand) {
        self.undo_stack.push(command);
        self.redo_stack.clear();

        // Enforce max_steps — drop the oldest commands.
        if self.undo_stack.len() > self.max_steps {
            let excess = self.undo_stack.len() - self.max_steps;
            self.undo_stack.drain(0..excess);
        }
    }

    /// Undoes the last command.
    ///
    /// `image` is the TiledImage to apply the undo to. The command is moved
    /// to the redo stack; if applying it fails, it is pushed back onto the
    /// undo stack so no history is lost.
    pub fn undo(&mut self, image: &mut TiledImage) -> ImageResult<()> {
        let command = self
            .undo_stack
            .pop()
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: kaleido_traits::HistoryError::NothingToUndo.to_string(),
            })?;

        if let Err(err) = command.apply_before(image) {
            // Roll back so a failed apply loses nothing.
            self.undo_stack.push(command);
            return Err(err);
        }
        self.redo_stack.push(command);
        Ok(())
    }

    /// Redoes the last undone command.
    ///
    /// `image` is the TiledImage to apply the redo to. The command is moved
    /// back to the undo stack; if applying it fails, it is pushed back onto
    /// the redo stack so no history is lost.
    pub fn redo(&mut self, image: &mut TiledImage) -> ImageResult<()> {
        let command = self
            .redo_stack
            .pop()
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: kaleido_traits::HistoryError::NothingToRedo.to_string(),
            })?;

        if let Err(err) = command.apply_after(image) {
            // Roll back so a failed apply loses nothing.
            self.redo_stack.push(command);
            return Err(err);
        }
        self.undo_stack.push(command);
        Ok(())
    }

    /// Returns whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns whether redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Returns the number of undo steps available.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Returns the number of redo steps available.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Returns the total payload memory usage of all stored snapshots
    /// (bytes) — before + after tile buffers only, no structural overhead.
    /// See [`TileSnapshotCommand::approx_bytes`] for an overhead-inclusive
    /// estimate.
    pub fn total_memory_bytes(&self) -> usize {
        self.undo_stack
            .iter()
            .chain(self.redo_stack.iter())
            .map(|cmd| cmd.total_bytes())
            .sum()
    }

    /// Rough estimate of retained memory: the keeper struct, each command
    /// (struct + payloads — occupied stack slots are covered by
    /// [`TileSnapshotCommand::approx_bytes`]), plus spare `Vec` capacity in
    /// both stacks. Allocator overhead is not counted.
    pub fn approx_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .undo_stack
                .iter()
                .chain(self.redo_stack.iter())
                .map(|cmd| cmd.approx_bytes())
                .sum::<usize>()
            + (self.undo_stack.capacity() + self.redo_stack.capacity()
                - self.undo_stack.len()
                - self.redo_stack.len())
                * std::mem::size_of::<TileSnapshotCommand>()
    }

    /// Returns the total number of tiles stored across all commands.
    pub fn total_tiles(&self) -> usize {
        self.undo_stack
            .iter()
            .chain(self.redo_stack.iter())
            .map(|cmd| cmd.tile_count())
            .sum()
    }

    /// Clears all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{Pixel, PixelFormat};

    #[test]
    fn test_tile_snapshot_command() {
        let mut image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();

        // Capture before state.
        let coords = vec![TileCoord::new(0, 0)];
        let mut command =
            TileSnapshotCommand::capture_before(&image, &coords, "test", "test operation");

        // Modify the image.
        image.set_pixel(10, 10, Pixel::new(200, 200, 200, 255));

        // Capture after state.
        command.capture_after(&image);

        // Verify the snapshot has different before/after.
        assert_eq!(command.tile_count(), 1);
        assert!(command.total_bytes() > 0);
    }

    #[test]
    fn test_tile_history_undo_redo() {
        let mut image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();
        let mut history = TileHistoryKeeper::new(10);

        // Capture and modify.
        let coords = vec![TileCoord::new(0, 0)];
        let mut command =
            TileSnapshotCommand::capture_before(&image, &coords, "brighten", "Brighten image");
        image.set_pixel(10, 10, Pixel::new(200, 200, 200, 255));
        command.capture_after(&image);
        history.push(command);

        // Verify the modification.
        assert_eq!(image.get_pixel(10, 10).r, 200);

        // Undo.
        history.undo(&mut image).unwrap();
        assert_eq!(image.get_pixel(10, 10).r, 100); // back to original

        // Redo.
        history.redo(&mut image).unwrap();
        assert_eq!(image.get_pixel(10, 10).r, 200); // modified again
    }

    #[test]
    fn test_tile_history_max_steps() {
        let mut history = TileHistoryKeeper::new(3);

        for i in 0..5 {
            let command = TileSnapshotCommand::new(
                vec![],
                format!("op {}", i),
                format!("operation {}", i),
            );
            history.push(command);
        }

        // Should only keep the last 3.
        assert_eq!(history.undo_count(), 3);
    }

    #[test]
    fn test_tile_history_memory_usage() {
        let mut image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();
        let mut history = TileHistoryKeeper::new(10);

        let coords = vec![TileCoord::new(0, 0)];
        let mut command =
            TileSnapshotCommand::capture_before(&image, &coords, "test", "test");
        image.set_pixel(10, 10, Pixel::new(200, 200, 200, 255));
        command.capture_after(&image);
        history.push(command);

        // Memory should be roughly 2x the tile size (before + after).
        let mem = history.total_memory_bytes();
        assert!(mem > 0);
        // A 128x128 image occupies one 256x256 tile buffer = 262144 bytes
        // per snapshot, 2 snapshots = 524288.
        assert_eq!(mem, 256 * 256 * 4 * 2);
    }

    #[test]
    fn test_tile_history_can_undo_redo() {
        let mut history = TileHistoryKeeper::new(10);
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        let command = TileSnapshotCommand::new(vec![], "test", "test");
        history.push(command);

        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_tile_history_clear() {
        let mut history = TileHistoryKeeper::new(10);
        let command = TileSnapshotCommand::new(vec![], "test", "test");
        history.push(command);

        assert!(history.can_undo());
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_apply_length_mismatch_returns_error() {
        let mut image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();
        let coords = vec![TileCoord::new(0, 0)];
        let mut command =
            TileSnapshotCommand::capture_before(&image, &coords, "test", "test");
        image.set_pixel(10, 10, Pixel::new(200, 200, 200, 255));
        command.capture_after(&image);

        // A different-format image has a differently sized tile buffer
        // (256×256×3 vs 256×256×4); applying must fail loudly instead of
        // silently skipping the restore.
        let mut other = TiledImage::with_color(128, 128, PixelFormat::Rgb8, Pixel::new(1, 2, 3, 255)).unwrap();
        assert!(command.apply_after(&mut other).is_err());
        assert!(command.apply_before(&mut other).is_err());
    }

    #[test]
    fn test_from_diff_records_tile_creation_and_deletion() {
        // before has tile (0,0); after has none — the snapshot records a
        // full `before` and an empty `after` (absent-tile convention).
        let before = TiledImage::with_color(8, 8, PixelFormat::Rgba8, Pixel::new(1, 2, 3, 255)).unwrap();
        let after = TiledImage::new(8, 8, PixelFormat::Rgba8);
        let command = TileSnapshotCommand::from_diff(&before, &after, "delete", "delete tile");

        assert_eq!(command.tile_count(), 1);
        assert!(!command.snapshots[0].before.is_empty());
        assert!(command.snapshots[0].after.is_empty());

        // Undo restores the tile (get_or_create + copy); redo of the
        // deletion is a no-op (undo cannot remove tiles — known limit).
        let mut restored = TiledImage::new(8, 8, PixelFormat::Rgba8);
        command.apply_before(&mut restored).unwrap();
        assert_eq!(
            restored.get_tile(0, 0).map(|t| t.data().to_vec()),
            before.get_tile(0, 0).map(|t| t.data().to_vec())
        );
        command.apply_after(&mut restored).unwrap();
        assert!(restored.get_tile(0, 0).is_some(), "redo of a tile deletion is a documented no-op");
    }

    #[test]
    fn test_max_steps_zero_keeps_nothing() {
        let mut history = TileHistoryKeeper::new(0);
        history.push(TileSnapshotCommand::new(vec![], "a", "a"));
        assert_eq!(history.undo_count(), 0);
        assert!(!history.can_undo());
    }

    #[test]
    fn test_approx_bytes_cover_total_bytes() {
        let mut image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();
        let coords = vec![TileCoord::new(0, 0)];
        let mut command =
            TileSnapshotCommand::capture_before(&image, &coords, "test", "test");
        image.set_pixel(10, 10, Pixel::new(200, 200, 200, 255));
        command.capture_after(&image);

        let mut history = TileHistoryKeeper::new(10);
        history.push(command);

        assert!(history.approx_memory_bytes() >= history.total_memory_bytes());
        assert!(history.total_memory_bytes() > 0);
    }

    #[test]
    fn test_undo_rolls_back_command_on_apply_error() {
        let mut image = TiledImage::with_color(8, 8, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();
        let coords = vec![TileCoord::new(0, 0)];
        let mut command =
            TileSnapshotCommand::capture_before(&image, &coords, "test", "test");
        image.set_pixel(1, 1, Pixel::new(200, 200, 200, 255));
        command.capture_after(&image);

        let mut history = TileHistoryKeeper::new(10);
        history.push(command);
        assert!(history.can_undo());

        // Applying to a different-format image fails; the command must be
        // restored to the undo stack so nothing is lost.
        let mut other = TiledImage::with_color(8, 8, PixelFormat::Rgb8, Pixel::new(1, 2, 3, 255)).unwrap();
        assert!(history.undo(&mut other).is_err());
        assert!(history.can_undo());
        assert_eq!(history.undo_count(), 1);
        assert_eq!(history.redo_count(), 0);
    }
}

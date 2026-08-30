//! Dirty-tile history for memory-efficient undo/redo.
//!
//! [`TileSnapshotCommand`] stores only the tiles that were modified by an
//! operation, not the entire image.  This makes undo/redo memory usage
//! proportional to the modified region rather than the full image size.


use kaleido_core::{ImageResult, TileCoord, TiledImage};
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
    /// operation for undo/redo.
    pub fn from_diff(
        before: &TiledImage,
        after: &TiledImage,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let mut snapshots = Vec::new();

        // Collect all tile coordinates present in either image.
        let mut all_coords: Vec<TileCoord> = before.tile_coords().collect();
        for coord in after.tile_coords() {
            if !all_coords.contains(&coord) {
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

    /// Returns the total bytes stored (before + after).
    pub fn total_bytes(&self) -> usize {
        self.snapshots
            .iter()
            .map(|s| s.before.len() + s.after.len())
            .sum()
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
    pub fn apply_after(&self, image: &mut TiledImage) -> ImageResult<()> {
        for snapshot in &self.snapshots {
            // An empty buffer means the tile was absent in the source image
            // (`from_diff` records `unwrap_or_default()` for missing tiles).
            // Writing it back would truncate the tile buffer to zero length
            // and panic on later pixel access — skip instead.
            if snapshot.after.is_empty() {
                continue;
            }
            let tile = image
                .get_or_create_tile(snapshot.coord.col, snapshot.coord.row);
            // Replace the tile data with the "after" state.
            let data = tile.data_mut();
            data.clear();
            data.extend_from_slice(&snapshot.after);
        }
        Ok(())
    }

    /// Applies the "before" state to a TiledImage (for undo).
    pub fn apply_before(&self, image: &mut TiledImage) -> ImageResult<()> {
        for snapshot in &self.snapshots {
            // See `apply_after` — an empty buffer means the tile did not
            // exist before the operation. Skipping keeps the current tile
            // intact (undo cannot remove tiles; tracked as a known limit).
            if snapshot.before.is_empty() {
                continue;
            }
            let tile = image
                .get_or_create_tile(snapshot.coord.col, snapshot.coord.row);
            // Replace the tile data with the "before" state.
            let data = tile.data_mut();
            data.clear();
            data.extend_from_slice(&snapshot.before);
        }
        Ok(())
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

        // Enforce max_steps limit.
        while self.undo_stack.len() > self.max_steps {
            self.undo_stack.remove(0);
        }
    }

    /// Undoes the last command.
    ///
    /// `image` is the TiledImage to apply the undo to.
    pub fn undo(&mut self, image: &mut TiledImage) -> ImageResult<()> {
        let command = self
            .undo_stack
            .pop()
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: kaleido_traits::HistoryError::NothingToUndo.to_string(),
            })?;

        command.apply_before(image)?;
        self.redo_stack.push(command);
        Ok(())
    }

    /// Redoes the last undone command.
    pub fn redo(&mut self, image: &mut TiledImage) -> ImageResult<()> {
        let command = self
            .redo_stack
            .pop()
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: kaleido_traits::HistoryError::NothingToRedo.to_string(),
            })?;

        command.apply_after(image)?;
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

    /// Returns the total memory usage of all stored snapshots (bytes).
    pub fn total_memory_bytes(&self) -> usize {
        self.undo_stack
            .iter()
            .chain(self.redo_stack.iter())
            .map(|cmd| cmd.total_bytes())
            .sum()
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
        // 128x128 RGBA8 = 65536 bytes per snapshot, 2 snapshots = 131072.
        assert_eq!(mem, 128 * 128 * 4 * 2);
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

}

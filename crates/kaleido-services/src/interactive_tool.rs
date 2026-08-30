//! Stroke executor for [`InteractiveTool`]s.
//!
//! [`InteractiveToolRunner`] sits between the host's canvas and the plugin.
//! It owns the parts an interactive tool must not have to think about:
//!
//! - **Undo**: it snapshots the touched tiles on `begin_stroke` and commits
//!   the dirty-tile command to the [`HistoryKeeper`] on `end_stroke`.
//! - **Working buffer**: it keeps a per-stroke copy of the image so the
//!   tool can mutate across many events without fighting the store lock.
//! - **Dirty tracking**: it accumulates the tiles the tool touched, for
//!   later incremental redraw.
//!
//! The host only needs to feed pointer events (already converted to image
//! space) and repaint when `store` emits `image_changed`.

use std::sync::Arc;

use kaleido_core::{ImageError, ImageResult, TileCoord, TiledImage};
use kaleido_traits::{HistoryKeeper, ImageStore, InteractiveTool, PointerEvent, ToolContext};

use crate::tile_history::TileSnapshotCommand;

// ---------------------------------------------------------------------------
// ActiveStroke
// ---------------------------------------------------------------------------

/// State for the stroke currently in progress.
struct ActiveStroke {
    /// Working copy of the image being edited.
    image: TiledImage,
    /// Undo command whose "before" state has already been captured.
    command: TileSnapshotCommand,
    /// Tiles touched so far by this stroke.
    dirty_tiles: Vec<TileCoord>,
}

// ---------------------------------------------------------------------------
// InteractiveToolRunner
// ---------------------------------------------------------------------------

/// Runs interactive tools against the current image with undo support.
pub struct InteractiveToolRunner {
    store: Arc<dyn ImageStore>,
    keeper: Arc<dyn HistoryKeeper>,
    active: Option<ActiveStroke>,
}

impl InteractiveToolRunner {
    /// Creates a runner bound to the given services.
    pub fn new(store: Arc<dyn ImageStore>, keeper: Arc<dyn HistoryKeeper>) -> Self {
        Self {
            store,
            keeper,
            active: None,
        }
    }

    /// Returns `true` while a stroke is in progress.
    pub fn is_stroke_active(&self) -> bool {
        self.active.is_some()
    }

    /// Returns the tiles touched by the active stroke (empty when idle).
    pub fn dirty_tiles(&self) -> &[TileCoord] {
        self.active
            .as_ref()
            .map(|s| s.dirty_tiles.as_slice())
            .unwrap_or(&[])
    }

    /// Starts a stroke: snapshots the undo state, then dispatches
    /// `on_mouse_down`.
    ///
    /// # Errors
    ///
    /// Returns an error if no image is loaded or a stroke is already
    /// active (call [`end_stroke`](Self::end_stroke) first).
    pub fn begin_stroke(
        &mut self,
        tool: &dyn InteractiveTool,
        event: &PointerEvent,
    ) -> ImageResult<()> {
        if self.active.is_some() {
            return Err(ImageError::OperationFailed {
                reason: "a stroke is already active".into(),
            });
        }

        let mut image = self.store.get_image()?.ok_or(ImageError::EmptyImage)?;
        let (doc_w, doc_h) = (image.width(), image.height());

        // Snapshot every existing tile. Tools that allocate new tiles are
        // covered by the after-state diff the command records.
        let coords: Vec<TileCoord> = image.tile_coords().collect();
        let command =
            TileSnapshotCommand::capture_before(&image, &coords, tool.name(), tool.description());

        let mut dirty_tiles = Vec::new();
        {
            let mut ctx = ToolContext::new(&mut image, doc_w, doc_h, &mut dirty_tiles);
            tool.on_mouse_down(&mut ctx, event)?;
        }

        // Publish the first dab immediately so the canvas can repaint.
        self.store.set_image(image.clone())?;

        self.active = Some(ActiveStroke {
            image,
            command,
            dirty_tiles,
        });
        Ok(())
    }

    /// Dispatches `on_mouse_drag` and republishes the image.
    ///
    /// No-op when no stroke is active.
    pub fn continue_stroke(
        &mut self,
        tool: &dyn InteractiveTool,
        event: &PointerEvent,
    ) -> ImageResult<()> {
        let Some(stroke) = self.active.as_mut() else {
            return Ok(());
        };
        let (doc_w, doc_h) = (stroke.image.width(), stroke.image.height());

        {
            let mut ctx = ToolContext::new(
                &mut stroke.image,
                doc_w,
                doc_h,
                &mut stroke.dirty_tiles,
            );
            tool.on_mouse_drag(&mut ctx, event)?;
        }

        self.store.set_image(stroke.image.clone())?;
        Ok(())
    }

    /// Dispatches `on_mouse_up`, captures the after-state and commits the
    /// stroke to the undo history.
    ///
    /// No-op when no stroke is active (so a stray pointer release is safe).
    pub fn end_stroke(
        &mut self,
        tool: &dyn InteractiveTool,
        event: &PointerEvent,
    ) -> ImageResult<()> {
        let Some(mut stroke) = self.active.take() else {
            return Ok(());
        };
        let (doc_w, doc_h) = (stroke.image.width(), stroke.image.height());

        {
            let mut ctx = ToolContext::new(
                &mut stroke.image,
                doc_w,
                doc_h,
                &mut stroke.dirty_tiles,
            );
            tool.on_mouse_up(&mut ctx, event)?;
        }

        let after = stroke.image.clone();
        self.store.set_image(after.clone())?;

        stroke.command.capture_after(&after);
        // `HistoryError` has no `Into<ImageError>`, so map it by hand.
        if let Err(err) = self.keeper.push(Box::new(stroke.command)) {
            return Err(ImageError::OperationFailed {
                reason: err.to_string(),
            });
        }

        // Post-processing hook. Its changes are written back to the store
        // but deliberately stay out of the recorded undo delta.
        {
            let mut ctx = ToolContext::new(
                &mut stroke.image,
                doc_w,
                doc_h,
                &mut stroke.dirty_tiles,
            );
            tool.on_stroke_end(&mut ctx)?;
            self.store.set_image(stroke.image.clone())?;
        }

        Ok(())
    }

    /// Abandons the active stroke, restoring the pre-stroke image.
    ///
    /// Nothing is added to the history.
    pub fn cancel_stroke(&mut self) -> ImageResult<()> {
        let Some(mut stroke) = self.active.take() else {
            return Ok(());
        };

        stroke.command.apply_before(&mut stroke.image)?;
        self.store.set_image(stroke.image.clone())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_codec_impl::FileCodecImpl;
    use crate::history_keeper_impl::HistoryKeeperImpl;
    use crate::image_store_impl::ImageStoreImpl;
    use cordis::Context;
    use kaleido_core::{Pixel, PixelFormat};
    use kaleido_traits::{Tool, ToolParams};

    /// A minimal brush: paints a filled square at each pointer position.
    struct SquareBrush {
        size: u32,
    }

    impl Tool for SquareBrush {
        fn name(&self) -> &str {
            "square_brush"
        }
        fn menu_path(&self) -> String {
            "绘画/方刷".into()
        }
        fn description(&self) -> String {
            "Paints squares".into()
        }
        fn apply(&self, _image: &mut TiledImage, _params: &ToolParams) -> ImageResult<()> {
            Ok(())
        }
    }

    impl InteractiveTool for SquareBrush {
        fn on_mouse_down(
            &self,
            ctx: &mut ToolContext,
            event: &PointerEvent,
        ) -> ImageResult<()> {
            self.dab(ctx, event)
        }

        fn on_mouse_drag(
            &self,
            ctx: &mut ToolContext,
            event: &PointerEvent,
        ) -> ImageResult<()> {
            self.dab(ctx, event)
        }
    }

    impl SquareBrush {
        fn dab(&self, ctx: &mut ToolContext, event: &PointerEvent) -> ImageResult<()> {
            let (cx, cy) = (event.x as u32, event.y as u32);
            let half = self.size / 2;
            for y in cy.saturating_sub(half)..=(cy + half) {
                for x in cx.saturating_sub(half)..=(cx + half) {
                    if x < ctx.image.width() && y < ctx.image.height() {
                        ctx.image.set_pixel(x, y, Pixel::rgb(255, 0, 0));
                    }
                }
            }
            ctx.mark_dirty(event.x, event.y);
            Ok(())
        }
    }

    fn setup() -> (
        Arc<dyn ImageStore>,
        Arc<dyn HistoryKeeper>,
        InteractiveToolRunner,
    ) {
        let ctx = Context::new();
        let codec = Arc::new(FileCodecImpl::new());
        let store: Arc<dyn ImageStore> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));
        let keeper: Arc<dyn HistoryKeeper> =
            Arc::new(HistoryKeeperImpl::new(Arc::downgrade(&store), ctx));

        let image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::rgb(0, 0, 255))
            .expect("test image");
        store.set_image(image).expect("seed image");

        let runner = InteractiveToolRunner::new(store.clone(), keeper.clone());
        (store, keeper, runner)
    }

    #[test]
    fn test_stroke_paints_and_records_undo() {
        let (store, keeper, mut runner) = setup();
        let brush = SquareBrush { size: 4 };

        assert!(!keeper.can_undo());

        runner
            .begin_stroke(&brush, &PointerEvent::down(64.0, 64.0))
            .unwrap();
        assert!(runner.is_stroke_active());

        runner
            .continue_stroke(&brush, &PointerEvent::down(70.0, 64.0))
            .unwrap();

        // Painted immediately, before the stroke ends.
        let mid = store.get_image().unwrap().unwrap();
        assert_eq!(mid.get_pixel(64, 64), Pixel::rgb(255, 0, 0));

        runner
            .end_stroke(&brush, &PointerEvent::down(70.0, 64.0))
            .unwrap();
        assert!(!runner.is_stroke_active());
        assert!(keeper.can_undo(), "stroke should be undoable");

        // Undo restores the original blue.
        keeper.undo().unwrap();
        let restored = store.get_image().unwrap().unwrap();
        assert_eq!(restored.get_pixel(64, 64), Pixel::rgb(0, 0, 255));

        // Redo paints it back.
        keeper.redo().unwrap();
        let redone = store.get_image().unwrap().unwrap();
        assert_eq!(redone.get_pixel(64, 64), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_dirty_tiles_are_tracked() {
        let (_store, _keeper, mut runner) = setup();
        let brush = SquareBrush { size: 8 };

        runner
            .begin_stroke(&brush, &PointerEvent::down(10.0, 10.0))
            .unwrap();

        let dirty = runner.dirty_tiles();
        assert!(!dirty.is_empty(), "brush should record dirty tiles");
        assert!(dirty.iter().any(|c| c.col == 0 && c.row == 0));

        runner
            .end_stroke(&brush, &PointerEvent::down(10.0, 10.0))
            .unwrap();
        assert!(runner.dirty_tiles().is_empty(), "cleared after stroke ends");
    }

    #[test]
    fn test_cancel_stroke_restores_without_history() {
        let (store, keeper, mut runner) = setup();
        let brush = SquareBrush { size: 4 };

        runner
            .begin_stroke(&brush, &PointerEvent::down(64.0, 64.0))
            .unwrap();
        runner.cancel_stroke().unwrap();

        assert!(!runner.is_stroke_active());
        assert!(!keeper.can_undo(), "cancel must not enter history");

        let image = store.get_image().unwrap().unwrap();
        assert_eq!(image.get_pixel(64, 64), Pixel::rgb(0, 0, 255));
    }

    #[test]
    fn test_double_begin_is_rejected() {
        let (_store, _keeper, mut runner) = setup();
        let brush = SquareBrush { size: 4 };

        runner
            .begin_stroke(&brush, &PointerEvent::down(10.0, 10.0))
            .unwrap();
        assert!(
            runner
                .begin_stroke(&brush, &PointerEvent::down(20.0, 20.0))
                .is_err()
        );
    }

    #[test]
    fn test_end_without_begin_is_noop() {
        let (_store, keeper, mut runner) = setup();
        let brush = SquareBrush { size: 4 };

        runner
            .end_stroke(&brush, &PointerEvent::down(10.0, 10.0))
            .unwrap();
        assert!(!keeper.can_undo());
    }
}

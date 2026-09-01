//! Canvas service for GPU-accelerated display.
//!
//! [`CanvasService`] handles all 2D display operations (zoom, pan, rotate)
//! on the GPU via GPUI's rendering backend.  It does not modify pixel
//! data — it only controls how existing pixel data is displayed.


use kaleido_core::TILE_SIZE;

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

/// Defines how the image is positioned within the canvas.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    /// Zoom factor (1.0 = 100%).
    pub zoom: f32,
    /// Pan offset in screen pixels.
    pub offset_x: f32,
    pub offset_y: f32,
    /// Rotation in radians.
    pub rotation: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            rotation: 0.0,
        }
    }
}

impl Viewport {
    /// Creates a viewport with the given zoom and offset.
    pub fn new(zoom: f32, offset_x: f32, offset_y: f32) -> Self {
        Self {
            zoom,
            offset_x,
            offset_y,
            ..Default::default()
        }
    }

    /// Returns the transform matrix for this viewport.
    /// Maps image coordinates to screen coordinates.
    pub fn transform_matrix(&self) -> [f32; 16] {
        // Column-major 4x4 matrix.
        let cos = self.rotation.cos();
        let sin = self.rotation.sin();
        let z = self.zoom;

        [
            z * cos,
            z * sin,
            0.0,
            0.0,
            -z * sin,
            z * cos,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            self.offset_x,
            self.offset_y,
            0.0,
            1.0,
        ]
    }

    /// Converts screen coordinates to image coordinates.
    pub fn screen_to_image(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let cos = self.rotation.cos();
        let sin = self.rotation.sin();
        let z = self.zoom;

        // Inverse of the transform (ignoring translation).
        let dx = screen_x - self.offset_x;
        let dy = screen_y - self.offset_y;

        let img_x = (cos * dx + sin * dy) / z;
        let img_y = (-sin * dx + cos * dy) / z;

        (img_x, img_y)
    }

    /// Converts image coordinates to screen coordinates.
    pub fn image_to_screen(&self, img_x: f32, img_y: f32) -> (f32, f32) {
        let cos = self.rotation.cos();
        let sin = self.rotation.sin();
        let z = self.zoom;

        let screen_x = (cos * img_x - sin * img_y) * z + self.offset_x;
        let screen_y = (sin * img_x + cos * img_y) * z + self.offset_y;

        (screen_x, screen_y)
    }

    /// Zooms centered on a screen point.
    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, factor: f32) {
        let (img_x, img_y) = self.screen_to_image(screen_x, screen_y);
        self.zoom = (self.zoom * factor).clamp(0.01, 100.0);
        // Adjust offset so the point under the cursor stays fixed.
        let (new_screen_x, new_screen_y) = self.image_to_screen(img_x, img_y);
        self.offset_x += screen_x - new_screen_x;
        self.offset_y += screen_y - new_screen_y;
    }
}

// ---------------------------------------------------------------------------
// RenderQuality
// ---------------------------------------------------------------------------

/// Controls the quality of progressive rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderQuality {
    /// Low quality, fast render (for real-time feedback).
    Low,
    /// Medium quality.
    Medium,
    /// Full quality (final render).
    High,
}

// ---------------------------------------------------------------------------
// CanvasService
// ---------------------------------------------------------------------------

/// GPU-accelerated canvas service.
///
/// This service manages the viewport and determines which tiles are visible.
/// The actual GPU rendering is delegated to the caller (desktop app).
pub struct CanvasService {
    viewport: Viewport,
    image_width: u32,
    image_height: u32,
    screen_width: u32,
    screen_height: u32,
}

impl CanvasService {
    /// Creates a new canvas service.
    pub fn new(
        image_width: u32,
        image_height: u32,
        screen_width: u32,
        screen_height: u32,
    ) -> Self {
        Self {
            viewport: Viewport::new(1.0, 0.0, 0.0),
            image_width,
            image_height,
            screen_width,
            screen_height,
        }
    }

    /// Returns the current viewport.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Returns a mutable reference to the viewport.
    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.viewport
    }

    /// Sets the viewport zoom.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.viewport.zoom = zoom.clamp(0.01, 100.0);
    }

    /// Sets the viewport offset.
    pub fn set_offset(&mut self, x: f32, y: f32) {
        self.viewport.offset_x = x;
        self.viewport.offset_y = y;
    }

    /// Sets the viewport rotation.
    pub fn set_rotation(&mut self, radians: f32) {
        self.viewport.rotation = radians;
    }

    /// Fits the image within the screen, centering it.
    pub fn fit_to_screen(&mut self) {
        let scale_x = self.screen_width as f32 / self.image_width as f32;
        let scale_y = self.screen_height as f32 / self.image_height as f32;
        self.viewport.zoom = scale_x.min(scale_y).min(1.0);
        self.viewport.offset_x =
            (self.screen_width as f32 - self.image_width as f32 * self.viewport.zoom) / 2.0;
        self.viewport.offset_y =
            (self.screen_height as f32 - self.image_height as f32 * self.viewport.zoom) / 2.0;
        self.viewport.rotation = 0.0;
    }

    /// Updates the screen size (e.g., on window resize).
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Updates the image size (e.g., when a new image is loaded).
    pub fn set_image_size(&mut self, width: u32, height: u32) {
        self.image_width = width;
        self.image_height = height;
    }

    /// Returns the visible region of the image in image coordinates.
    ///
    /// All four screen corners are mapped back into image space and the
    /// axis-aligned bounding box is taken. Under rotation this is a
    /// conservative over-approximation (it may include slightly more than
    /// the exact rotated visible quad, but never misses it).
    pub fn visible_image_rect(&self) -> (u32, u32, u32, u32) {
        let corners = [
            self.viewport.screen_to_image(0.0, 0.0),
            self.viewport
                .screen_to_image(self.screen_width as f32, 0.0),
            self.viewport
                .screen_to_image(0.0, self.screen_height as f32),
            self.viewport
                .screen_to_image(self.screen_width as f32, self.screen_height as f32),
        ];

        let min_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .max(0.0);
        let min_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .max(0.0);
        let max_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(self.image_width as f32);
        let max_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(self.image_height as f32);

        let x = min_x as u32;
        let y = min_y as u32;
        let width = (max_x - min_x).max(0.0) as u32;
        let height = (max_y - min_y).max(0.0) as u32;

        (x, y, width, height)
    }

    /// Returns the tile coordinates that are currently visible.
    pub fn visible_tile_coords(&self) -> Vec<(u32, u32)> {
        let (vx, vy, vw, vh) = self.visible_image_rect();
        if vw == 0 || vh == 0 {
            return Vec::new();
        }

        let tile_size = TILE_SIZE;
        let start_col = vx / tile_size;
        let start_row = vy / tile_size;
        let end_col = ((vx + vw + tile_size - 1) / tile_size).min(
            (self.image_width + tile_size - 1) / tile_size,
        );
        let end_row = ((vy + vh + tile_size - 1) / tile_size).min(
            (self.image_height + tile_size - 1) / tile_size,
        );

        let mut coords = Vec::new();
        for row in start_row..end_row {
            for col in start_col..end_col {
                coords.push((col, row));
            }
        }
        coords
    }

    /// Returns whether a tile at the given coordinate is visible.
    pub fn is_tile_visible(&self, col: u32, row: u32) -> bool {
        let tile_size = TILE_SIZE;
        let tile_x = col * tile_size;
        let tile_y = row * tile_size;
        let tile_right = tile_x + tile_size;
        let tile_bottom = tile_y + tile_size;

        let (vx, vy, vw, vh) = self.visible_image_rect();
        let vrx = vx + vw;
        let vry = vy + vh;

        // AABB overlap test.
        tile_x < vrx && tile_right > vx && tile_y < vry && tile_bottom > vy
    }

    /// Pans the viewport by the given screen-space delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.viewport.offset_x += dx;
        self.viewport.offset_y += dy;
    }

    /// Rotates the viewport by the given angle (radians).
    pub fn rotate(&mut self, radians: f32) {
        self.viewport.rotation += radians;
    }

    /// Returns the screen-space bounding box of the image.
    pub fn image_screen_bounds(&self) -> (f32, f32, f32, f32) {
        let corners = [
            self.viewport.image_to_screen(0.0, 0.0),
            self.viewport.image_to_screen(self.image_width as f32, 0.0),
            self.viewport
                .image_to_screen(self.image_width as f32, self.image_height as f32),
            self.viewport.image_to_screen(0.0, self.image_height as f32),
        ];

        let min_x = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_x = corners.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);

        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

// ---------------------------------------------------------------------------
// ProgressiveRenderer
// ---------------------------------------------------------------------------

/// Manages progressive rendering quality levels.
pub struct ProgressiveRenderer {
    current_quality: RenderQuality,
    target_quality: RenderQuality,
    frames_at_current: u32,
}

impl ProgressiveRenderer {
    pub fn new() -> Self {
        Self {
            current_quality: RenderQuality::Low,
            target_quality: RenderQuality::High,
            frames_at_current: 0,
        }
    }

    /// Returns the current render quality.
    pub fn current_quality(&self) -> RenderQuality {
        self.current_quality
    }

    /// Sets the target quality (final quality to converge to).
    pub fn set_target_quality(&mut self, quality: RenderQuality) {
        self.target_quality = quality;
        self.frames_at_current = 0;
    }

    /// Called when a new frame is rendered.
    /// Returns the quality to use for this frame.
    pub fn next_frame(&mut self) -> RenderQuality {
        self.frames_at_current += 1;

        // Progressive refinement: start at low quality, gradually increase.
        self.current_quality = match self.current_quality {
            RenderQuality::Low => {
                // After 2 low-quality frames (frames 1 and 2), move to medium on frame 3.
                if self.frames_at_current >= 3 {
                    self.frames_at_current = 0;
                    RenderQuality::Medium
                } else {
                    RenderQuality::Low
                }
            }
            RenderQuality::Medium => {
                // After 3 medium-quality frames, move to high.
                if self.frames_at_current >= 3 {
                    self.frames_at_current = 0;
                    self.target_quality
                } else {
                    RenderQuality::Medium
                }
            }
            RenderQuality::High => RenderQuality::High,
        };

        self.current_quality
    }

    /// Resets the progressive renderer (e.g., on new interaction).
    pub fn reset(&mut self) {
        self.current_quality = RenderQuality::Low;
        self.frames_at_current = 0;
    }
}

impl Default for ProgressiveRenderer {
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

    #[test]
    fn test_viewport_default() {
        let vp = Viewport::default();
        assert_eq!(vp.zoom, 1.0);
        assert_eq!(vp.offset_x, 0.0);
        assert_eq!(vp.offset_y, 0.0);
        assert_eq!(vp.rotation, 0.0);
    }

    #[test]
    fn test_viewport_transform_roundtrip() {
        let vp = Viewport::new(2.0, 100.0, 50.0);
        let (sx, sy) = vp.image_to_screen(10.0, 20.0);
        let (ix, iy) = vp.screen_to_image(sx, sy);
        assert!((ix - 10.0).abs() < 0.001);
        assert!((iy - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_viewport_zoom_at() {
        let mut vp = Viewport::new(1.0, 0.0, 0.0);
        let (sx, sy) = vp.image_to_screen(100.0, 100.0);
        vp.zoom_at(sx, sy, 2.0);
        assert_eq!(vp.zoom, 2.0);
        // The point under the cursor should remain fixed.
        let (sx2, sy2) = vp.image_to_screen(100.0, 100.0);
        assert!((sx - sx2).abs() < 0.001);
        assert!((sy - sy2).abs() < 0.001);
    }

    #[test]
    fn test_viewport_screen_to_image_zero() {
        let vp = Viewport::default();
        let (x, y) = vp.screen_to_image(0.0, 0.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_canvas_new() {
        let canvas = CanvasService::new(800, 600, 1024, 768);
        assert_eq!(canvas.image_width, 800);
        assert_eq!(canvas.image_height, 600);
    }

    #[test]
    fn test_canvas_fit_to_screen() {
        let mut canvas = CanvasService::new(800, 600, 1024, 768);
        canvas.fit_to_screen();
        // Image should fit within screen.
        let bounds = canvas.image_screen_bounds();
        assert!(bounds.2 <= 1024.0); // width fits
        assert!(bounds.3 <= 768.0); // height fits
    }

    #[test]
    fn test_canvas_visible_tile_coords() {
        let canvas = CanvasService::new(512, 512, 1024, 768);
        // 512x512 image = 4 tiles of 256x256, all visible.
        let coords = canvas.visible_tile_coords();
        assert_eq!(coords.len(), 4);
    }

    #[test]
    fn test_canvas_visible_tile_coords_partial() {
        // 300x300 image = 9 tiles (3x3), but only some visible.
        let canvas = CanvasService::new(300, 300, 200, 200);
        let coords = canvas.visible_tile_coords();
        // Only tiles overlapping the visible region should be returned.
        assert!(!coords.is_empty());
    }

    #[test]
    fn test_canvas_is_tile_visible() {
        let canvas = CanvasService::new(512, 512, 1024, 768);
        assert!(canvas.is_tile_visible(0, 0));
        assert!(canvas.is_tile_visible(1, 1));
        // Tile (2, 2) is outside the image.
        assert!(!canvas.is_tile_visible(2, 2));
    }

    #[test]
    fn test_canvas_pan() {
        let mut canvas = CanvasService::new(256, 256, 1024, 768);
        let initial_offset = canvas.viewport().offset_x;
        canvas.pan(10.0, -5.0);
        assert_eq!(canvas.viewport().offset_x, initial_offset + 10.0);
        assert_eq!(canvas.viewport().offset_y, -5.0);
    }

    #[test]
    fn test_canvas_visible_rect_rotated_is_conservative() {
        // With rotation the visible image region is a rotated quad; the
        // reported rect must stay within the image and cover the axis-
        // aligned unrotated case.
        let mut canvas = CanvasService::new(512, 512, 1024, 768);
        canvas.set_rotation(0.5);
        let (x, y, w, h) = canvas.visible_image_rect();
        // Bounded by the image.
        assert!(x + w <= 512);
        assert!(y + h <= 512);
        // Non-empty and conservative: the unrotated visible tiles are a
        // subset of the rotated ones.
        let rotated = canvas.visible_tile_coords();
        canvas.set_rotation(0.0);
        let plain = canvas.visible_tile_coords();
        for coord in &plain {
            assert!(rotated.contains(coord), "missed tile {coord:?} under rotation");
        }
    }

    #[test]
    fn test_canvas_rotate() {
        let mut canvas = CanvasService::new(256, 256, 1024, 768);
        canvas.rotate(std::f32::consts::PI / 2.0);
        assert!((canvas.viewport().rotation - std::f32::consts::PI / 2.0).abs() < 0.001);
    }

    #[test]
    fn test_progressive_renderer() {
        let mut renderer = ProgressiveRenderer::new();
        assert_eq!(renderer.current_quality(), RenderQuality::Low);

        // First frame: low quality.
        assert_eq!(renderer.next_frame(), RenderQuality::Low);
        // Second frame: low quality.
        assert_eq!(renderer.next_frame(), RenderQuality::Low);
        // Third frame: medium quality.
        assert_eq!(renderer.next_frame(), RenderQuality::Medium);
    }

    #[test]
    fn test_progressive_renderer_reset() {
        let mut renderer = ProgressiveRenderer::new();
        renderer.next_frame();
        renderer.next_frame();
        renderer.reset();
        assert_eq!(renderer.current_quality(), RenderQuality::Low);
    }

    #[test]
    fn test_viewport_transform_matrix_identity() {
        let vp = Viewport::default();
        let m = vp.transform_matrix();
        // Identity matrix (with translation 0).
        assert_eq!(m[0], 1.0); // scale x
        assert_eq!(m[5], 1.0); // scale y
        assert_eq!(m[12], 0.0); // translation x
        assert_eq!(m[13], 0.0); // translation y
    }
}

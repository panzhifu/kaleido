//! Viewport state (zoom, pan, rotation).

#[derive(Debug, Clone)]
pub struct ViewportState {
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub rotation: f32,
    pub image_width: u32,
    pub image_height: u32,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            rotation: 0.0,
            image_width: 0,
            image_height: 0,
        }
    }
}

impl ViewportState {
    pub fn set_image_size(&mut self, width: u32, height: u32) {
        self.image_width = width;
        self.image_height = height;
    }

    pub fn fit_to_screen(&mut self, screen_w: f32, screen_h: f32) {
        if self.image_width == 0 || self.image_height == 0 { return; }
        let scale_x = screen_w / self.image_width as f32;
        let scale_y = screen_h / self.image_height as f32;
        self.zoom = scale_x.min(scale_y).min(1.0);
        self.offset_x = (screen_w - self.image_width as f32 * self.zoom) / 2.0;
        self.offset_y = (screen_h - self.image_height as f32 * self.zoom) / 2.0;
        self.rotation = 0.0;
    }

    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, factor: f32) {
        let img_x = (screen_x - self.offset_x) / self.zoom;
        let img_y = (screen_y - self.offset_y) / self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.01, 100.0);
        self.offset_x = screen_x - img_x * self.zoom;
        self.offset_y = screen_y - img_y * self.zoom;
    }

    pub fn reset(&mut self, image_width: u32, image_height: u32) {
        self.image_width = image_width;
        self.image_height = image_height;
        self.zoom = 1.0;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.rotation = 0.0;
    }
}

//! Canvas component: renders the current image and routes pointer input
//! to the active [`InteractiveTool`].
//!
//! The canvas reads pixels straight from the [`ImageStore`], so any change
//! (tool execution, undo/redo, brush stroke) shows up on the next paint.
//! Pointer events are converted from window space to image space before
//! being handed to the tool, and the [`InteractiveToolRunner`] takes care
//! of undo and republishing.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::*;

use image::{ImageBuffer, Frame, Rgba};
use smallvec::SmallVec;

use kaleido_core::{PixelFormat, TiledImage};
use kaleido_services::InteractiveToolRunner;
use kaleido_traits::{
    HistoryKeeper, ImageStore, InteractiveTool, KeyCode, KeyEvent, KeyModifiers, Modifiers,
    PointerButtons, PointerEvent, PointerKind,
};

// Layout constants matching the shell layout in `app.rs`. They are needed
// to map a window-space mouse position back to image space, because this
// gpui version exposes no generic "element bounds" lookup in listeners.
const TOOLBAR_WIDTH: f32 = 48.0;
const MODE_BAR_HEIGHT: f32 = 36.0;
const STATUS_BAR_HEIGHT: f32 = 24.0;
const RIGHT_PANEL_WIDTH: f32 = 240.0;

pub struct Canvas {
    store: Arc<dyn ImageStore>,
    /// Stroke executor shared with the event listeners (they capture
    /// clones, and listeners must be `Fn`, not `FnMut`).
    runner: Rc<RefCell<InteractiveToolRunner>>,
    /// The interactive tool currently receiving pointer events.
    tool: Option<Arc<dyn InteractiveTool>>,
    #[allow(dead_code)]
    zoom: f32,
    #[allow(dead_code)]
    offset_x: f32,
    #[allow(dead_code)]
    offset_y: f32,
}

impl Canvas {
    pub fn new(
        store: Arc<dyn ImageStore>,
        keeper: Arc<dyn HistoryKeeper>,
        _cx: &mut Context<Self>,
    ) -> Self {
        let runner = InteractiveToolRunner::new(store.clone(), keeper);
        Self {
            store,
            runner: Rc::new(RefCell::new(runner)),
            tool: None,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    /// Sets the interactive tool that receives pointer events.
    ///
    /// Calls `on_deactivate` on the previous tool (if any) and
    /// `on_activate` on the new tool.
    pub fn set_tool(&mut self, tool: Arc<dyn InteractiveTool>) {
        // Deactivate the previous tool.
        if let Some(old) = self.tool.take() {
            self.runner.borrow().deactivate(old.as_ref());
        }
        // Activate the new tool.
        self.runner.borrow().activate(tool.as_ref());
        self.tool = Some(tool);
    }

    /// Clears the active interactive tool.
    pub fn clear_tool(&mut self) {
        if let Some(old) = self.tool.take() {
            self.runner.borrow().deactivate(old.as_ref());
        }
    }

    #[allow(dead_code)]
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    #[allow(dead_code)]
    pub fn set_offset(&mut self, x: f32, y: f32) {
        self.offset_x = x;
        self.offset_y = y;
    }

    #[allow(dead_code)]
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Returns the size of the current image, if any.
    pub fn image_size(&self) -> Option<(u32, u32)> {
        self.store
            .get_image()
            .ok()
            .flatten()
            .map(|img| (img.width(), img.height()))
    }

    /// Convert TiledImage to an Arc<RenderImage> for display.
    fn render_image(image: &TiledImage) -> Option<Arc<RenderImage>> {
        let width = image.width();
        let height = image.height();

        if width == 0 || height == 0 {
            return None;
        }

        let bytes = match image.format() {
            PixelFormat::Rgba8 => image.to_raw_vec(),
            _ => image.to_rgba_vec(),
        };

        if bytes.len() < (width * height * 4) as usize {
            return None;
        }

        let image_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, bytes)?;

        let frame = Frame::new(image_buffer);
        let render_image = RenderImage::new(SmallVec::from_elem(frame, 1));

        Some(Arc::new(render_image))
    }
}

/// Converts a window-space position into image space and feeds it to the
/// active tool.
///
/// `size` is the current image size; `zoom` the canvas zoom factor.
#[allow(clippy::too_many_arguments)]
fn dispatch_pointer(
    runner: &Rc<RefCell<InteractiveToolRunner>>,
    tool: &Option<Arc<dyn InteractiveTool>>,
    size: Option<(u32, u32)>,
    zoom: f32,
    position: Point<Pixels>,
    kind: PointerKind,
    window: &mut Window,
) {
    let Some(tool) = tool.as_ref() else {
        return;
    };
    let Some((img_w, img_h)) = size else {
        return;
    };

    // Window → image coordinates. The image is centred inside the canvas
    // area, so subtract that offset before undoing the zoom.
    // `Pixels` keeps its field private; convert through `From<Pixels>`.
    let viewport = window.viewport_size();
    let area_w = f32::from(viewport.width) - TOOLBAR_WIDTH - RIGHT_PANEL_WIDTH;
    let area_h = f32::from(viewport.height) - MODE_BAR_HEIGHT - STATUS_BAR_HEIGHT;
    let display_w = img_w as f32 * zoom;
    let display_h = img_h as f32 * zoom;
    let origin_x = TOOLBAR_WIDTH + (area_w - display_w) / 2.0;
    let origin_y = MODE_BAR_HEIGHT + (area_h - display_h) / 2.0;

    let x = (f32::from(position.x) - origin_x) / zoom;
    let y = (f32::from(position.y) - origin_y) / zoom;

    // Ignore drawing outside the image, but always let a release through
    // so a stroke that wanders off-canvas still gets committed.
    let inside = x >= 0.0 && y >= 0.0 && x < img_w as f32 && y < img_h as f32;
    if !inside && kind != PointerKind::Up {
        return;
    }

    let event = PointerEvent::new(
        x,
        y,
        1.0,
        PointerButtons::new(PointerButtons::PRIMARY),
        Modifiers::new(0),
        kind,
    );

    let Ok(mut runner) = runner.try_borrow_mut() else {
        return;
    };
    match kind {
        PointerKind::Down => {
            let _ = runner.begin_stroke(tool.as_ref(), &event);
        }
        PointerKind::Drag => {
            if runner.is_stroke_active() {
                let _ = runner.continue_stroke(tool.as_ref(), &event);
            }
        }
        PointerKind::Up => {
            let _ = runner.end_stroke(tool.as_ref(), &event);
        }
    }
    drop(runner);

    // Repaint so the stroke (and the status bar counters) update.
    window.refresh();
}

/// Converts a GPUI keystroke into our `KeyCode`.
fn gpui_key_to_keycode(key: &str, key_char: &Option<String>) -> KeyCode {
    // Prefer key_char for printable characters (handles shift, IME).
    if let Some(ch) = key_char {
        if ch.len() == 1 {
            return KeyCode::Char(ch.chars().next().unwrap().to_ascii_lowercase());
        }
    }
    // Fall back to the physical key name.
    match key {
        "escape" => KeyCode::Escape,
        "enter" | "return" => KeyCode::Enter,
        "backspace" | "delete" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Space,
        "arrow-up" => KeyCode::ArrowUp,
        "arrow-down" => KeyCode::ArrowDown,
        "arrow-left" => KeyCode::ArrowLeft,
        "arrow-right" => KeyCode::ArrowRight,
        "[" => KeyCode::LeftBracket,
        "]" => KeyCode::RightBracket,
        "=" | "plus" => KeyCode::Plus,
        "-" | "underscore" => KeyCode::Minus,
        _ => {
            // Single ASCII character.
            if key.len() == 1 {
                let c = key.chars().next().unwrap();
                if c.is_ascii_alphabetic() || c.is_ascii_digit() {
                    KeyCode::Char(c.to_ascii_lowercase())
                } else {
                    KeyCode::Unknown
                }
            } else {
                KeyCode::Unknown
            }
        }
    }
}

/// Converts GPUI modifiers to our `KeyModifiers`.
fn gpui_modifiers_to_keymodifiers(m: &gpui::Modifiers) -> KeyModifiers {
    let mut out = KeyModifiers::new(0);
    if m.shift {
        out.insert(KeyModifiers::SHIFT);
    }
    if m.control {
        out.insert(KeyModifiers::CTRL);
    }
    if m.alt {
        out.insert(KeyModifiers::ALT);
    }
    if m.platform {
        out.insert(KeyModifiers::COMMAND);
    }
    out
}

/// Dispatches a keyboard event to the active tool.
fn dispatch_keyboard(
    runner: &Rc<RefCell<InteractiveToolRunner>>,
    tool: &Option<Arc<dyn InteractiveTool>>,
    key_event: &KeyEvent,
    is_down: bool,
) {
    let Some(tool) = tool.as_ref() else {
        return;
    };
    let Ok(runner) = runner.try_borrow() else {
        return;
    };
    if is_down {
        let _ = runner.key_down(tool.as_ref(), key_event);
    } else {
        let _ = runner.key_up(tool.as_ref(), key_event);
    }
}

impl Render for Canvas {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Cheap clone: TiledImage shares tile buffers via Arc.
        let image = self.store.get_image().ok().flatten();
        let size = image.as_ref().map(|img| (img.width(), img.height()));
        let zoom = self.zoom;
        let render_image = image.as_ref().and_then(Self::render_image);

        let runner = self.runner.clone();
        let tool = self.tool.clone();

        div()
            .id("canvas-surface")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, {
                let (runner, tool) = (runner.clone(), tool.clone());
                move |event: &MouseDownEvent, window: &mut Window, _cx: &mut App| {
                    dispatch_pointer(
                        &runner,
                        &tool,
                        size,
                        zoom,
                        event.position,
                        PointerKind::Down,
                        window,
                    );
                }
            })
            .on_mouse_move({
                let (runner, tool) = (runner.clone(), tool.clone());
                move |event: &MouseMoveEvent, window: &mut Window, _cx: &mut App| {
                    dispatch_pointer(
                        &runner,
                        &tool,
                        size,
                        zoom,
                        event.position,
                        PointerKind::Drag,
                        window,
                    );
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let (runner, tool) = (runner.clone(), tool.clone());
                move |event: &MouseUpEvent, window: &mut Window, _cx: &mut App| {
                    dispatch_pointer(
                        &runner,
                        &tool,
                        size,
                        zoom,
                        event.position,
                        PointerKind::Up,
                        window,
                    );
                }
            })
            // Fires when the button is released outside the canvas, so a
            // stroke never gets stuck in the active state.
            .on_mouse_up_out(MouseButton::Left, {
                let (runner, tool) = (runner.clone(), tool.clone());
                move |event: &MouseUpEvent, window: &mut Window, _cx: &mut App| {
                    dispatch_pointer(
                        &runner,
                        &tool,
                        size,
                        zoom,
                        event.position,
                        PointerKind::Up,
                        window,
                    );
                }
            })
            // Keyboard events: convert GPUI keystrokes to our KeyEvent and
            // dispatch to the active tool. We use Div-level handlers which
            // are called during the paint phase.
            .on_key_down({
                let (runner, tool) = (runner.clone(), tool.clone());
                move |event: &gpui::KeyDownEvent, _window, _cx| {
                    let ke = &event.keystroke;
                    let code = gpui_key_to_keycode(&ke.key, &ke.key_char);
                    let modifiers = gpui_modifiers_to_keymodifiers(&ke.modifiers);
                    let key_event = KeyEvent::new(code, modifiers);
                    dispatch_keyboard(&runner, &tool, &key_event, true);
                }
            })
            .on_key_up({
                let (runner, tool) = (runner.clone(), tool.clone());
                move |event: &gpui::KeyUpEvent, _window, _cx| {
                    let ke = &event.keystroke;
                    let code = gpui_key_to_keycode(&ke.key, &ke.key_char);
                    let modifiers = gpui_modifiers_to_keymodifiers(&ke.modifiers);
                    let key_event = KeyEvent::new(code, modifiers);
                    dispatch_keyboard(&runner, &tool, &key_event, false);
                }
            })
            .child(if let Some(render_image) = render_image {
                let (w, h) = size
                    .map(|(iw, ih)| (iw as f32 * zoom, ih as f32 * zoom))
                    .unwrap_or((0.0, 0.0));
                div()
                    .child(
                        img(ImageSource::Render(render_image))
                            .w(px(w.max(1.0)))
                            .h(px(h.max(1.0))),
                    )
                    .into_any_element()
            } else {
                div()
                    .text_color(gpui::rgb(0x666666))
                    .text_size(px(14.))
                    .child("Canvas - 打开文件开始编辑")
                    .into_any_element()
            })
    }
}

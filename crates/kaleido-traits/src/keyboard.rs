//! Keyboard input types for interactive tools.
//!
//! Interactive tools receive keyboard events alongside pointer events,
//! so plugins can respond to shortcuts, brush-size adjustments, and
//! modifier keys (Shift/Esc/[ ] etc.).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// KeyModifiers
// ---------------------------------------------------------------------------

/// Bitmask of modifier keys held during a keyboard or pointer event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    pub const SHIFT: u8 = 1 << 0;
    pub const CTRL: u8 = 1 << 1;
    pub const ALT: u8 = 1 << 2;
    pub const COMMAND: u8 = 1 << 3; // macOS Super / Windows key

    pub const fn new(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn shift(&self) -> bool {
        self.0 & Self::SHIFT != 0
    }

    pub const fn ctrl(&self) -> bool {
        self.0 & Self::CTRL != 0
    }

    pub const fn alt(&self) -> bool {
        self.0 & Self::ALT != 0
    }

    pub const fn command(&self) -> bool {
        self.0 & Self::COMMAND != 0
    }

    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, bit: u8) {
        self.0 |= bit;
    }
}

// ---------------------------------------------------------------------------
// KeyCode
// ---------------------------------------------------------------------------

/// A physical key identifier (printable character or named key).
///
/// This mirrors the subset of keys an image editor typically cares about.
/// Plugins match on the variants they need and ignore the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    /// A printable character (normalised to lowercase).
    Char(char),
    /// Escape — commonly used to cancel a stroke or dismiss a preview.
    Escape,
    /// Enter / Return — confirm a preview, commit a text entry.
    Enter,
    /// Backspace / Delete.
    Backspace,
    /// Arrow keys.
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// Tab / Shift-Tab.
    Tab,
    /// Space bar.
    Space,
    /// `[` and `]` — the de-facto brush-size keys.
    LeftBracket,
    RightBracket,
    /// Plus / minus — zoom, brush size, etc.
    Plus,
    Minus,
    /// Unknown or unsupported key.
    Unknown,
}

// ---------------------------------------------------------------------------
// KeyEvent
// ---------------------------------------------------------------------------

/// A keyboard event delivered to an interactive tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    /// Which key this event is for.
    pub code: KeyCode,
    /// Modifier keys held at the time of the event.
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    /// Creates a new key event.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Creates a simple key event with no modifiers.
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::new(0),
        }
    }
}

// ---------------------------------------------------------------------------
// KeyState — query interface for the host
// ---------------------------------------------------------------------------

/// Implemented by the host so interactive tools can query the current
/// modifier state at any time (not just inside an event callback).
pub trait KeyState {
    /// Returns the currently-held modifier keys.
    fn modifiers(&self) -> KeyModifiers;

    /// Returns `true` if the given key is currently held down.
    fn is_held(&self, code: KeyCode) -> bool;
}

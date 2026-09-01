//! Plugin capabilities — unified API for all plugin development.
//!
//! This file defines the capabilities available to all plugins (internal and external).
//! Plugins implement these capabilities through traits (Rust) or ABI functions (WASM).
//!
//! # Capability Overview
//!
//! | Capability | Trait (Rust) | WASM ABI | Description |
//! |------------|--------------|----------|-------------|
//! | Format Codec | `FormatCodec` | `format_decode` / `format_encode` | Decode/encode image formats |
//! | Document | `DataService` | `document_open` / `document_save` | Open/save documents |
//! | History | `HistoryService` | `history_undo` / `history_redo` | Undo/redo operations |
//!
//! # Plugin Types
//!
//! ## Internal Plugins (Rust)
//!
//! Implement traits directly:
//!
//! ```ignore
//! pub struct MyCodec;
//!
//! impl FormatCodec for MyCodec {
//!     fn format(&self) -> ImageFormat { ImageFormat::custom("myformat") }
//!     fn load(&self, path: &Path) -> ImageResult<TiledImage> { /* ... */ }
//!     fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()> { /* ... */ }
//!     // ...
//! }
//! ```
//!
//! ## External Plugins (WASM)
//!
//! Export ABI functions:
//!
//! ```c
//! // WASM module exports
//! int32_t format_decode(const char* path, int32_t path_len);
//! int32_t format_encode(const char* path, int32_t path_len,
//!                       const uint8_t* data, int32_t data_len,
//!                       int32_t width, int32_t height);
//! ```

// ── WASM ABI Reference ──────────────────────────────────────────────────

/// WASM ABI function signatures for format codec plugins.
///
/// External WASM plugins export these functions to implement format codec capability.
pub mod abi {
    /// Decodes an image file.
    ///
    /// # Parameters
    /// - `path_ptr`: pointer to the file path string in WASM memory
    /// - `path_len`: length of the file path string
    ///
    /// # Returns
    /// - Non-negative: buffer handle containing decoded image data
    /// - Negative: error code
    pub const FORMAT_DECODE: &str = "format_decode";

    /// Encodes an image to a file.
    ///
    /// # Parameters
    /// - `path_ptr`: pointer to the output file path string
    /// - `path_len`: length of the output file path string
    /// - `data_ptr`: pointer to the RGBA pixel data
    /// - `data_len`: length of the pixel data
    /// - `width`: image width
    /// - `height`: image height
    ///
    /// # Returns
    /// - 0: success
    /// - Negative: error code
    pub const FORMAT_ENCODE: &str = "format_encode";

    /// Returns the format name.
    ///
    /// # Parameters
    /// - `index`: codec index (for plugins that support multiple formats)
    ///
    /// # Returns
    /// - Pointer to a null-terminated string in WASM memory
    pub const FORMAT_GET_NAME: &str = "format_get_name";

    /// Returns the supported extensions.
    ///
    /// # Parameters
    /// - `index`: codec index
    ///
    /// # Returns
    /// - Pointer to a null-terminated string (e.g. `"avif,heif"`)
    pub const FORMAT_GET_EXTS: &str = "format_get_exts";

    // ── Document ABI ────────────────────────────────────────────────────

    /// Opens a document from a file.
    pub const DOCUMENT_OPEN: &str = "document_open";

    /// Saves the current document to a file.
    pub const DOCUMENT_SAVE: &str = "document_save";

    /// Returns the current document handle.
    pub const DOCUMENT_GET: &str = "document_get";

    // ── History ABI ─────────────────────────────────────────────────────

    /// Undoes the last operation.
    pub const HISTORY_UNDO: &str = "history_undo";

    /// Redoes the last undone operation.
    pub const HISTORY_REDO: &str = "history_redo";

    /// Pushes a new history entry.
    pub const HISTORY_PUSH: &str = "history_push";
}

// ── Buffer Layout ────────────────────────────────────────────────────────

/// Buffer layout for data exchange between host and WASM.
///
/// ```text
/// [ptr: i32, len: i32, width: i32, height: i32]
///   │       │         │          │
///   │       │         │          └── image height
///   │       │         └── image width
///   │       └── data length in bytes
///   └── pointer to data in WASM memory
/// ```
pub mod buffer {
    /// Offset of the pointer field in the buffer.
    pub const PTR_OFFSET: usize = 0;
    /// Offset of the length field in the buffer.
    pub const LEN_OFFSET: usize = 4;
    /// Offset of the width field in the buffer.
    pub const WIDTH_OFFSET: usize = 8;
    /// Offset of the height field in the buffer.
    pub const HEIGHT_OFFSET: usize = 12;
    /// Total size of the buffer header in bytes.
    pub const HEADER_SIZE: usize = 16;
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_constants() {
        assert_eq!(abi::FORMAT_DECODE, "format_decode");
        assert_eq!(abi::FORMAT_ENCODE, "format_encode");
        assert_eq!(abi::DOCUMENT_OPEN, "document_open");
        assert_eq!(abi::HISTORY_UNDO, "history_undo");
    }

    #[test]
    fn test_buffer_layout() {
        assert_eq!(buffer::PTR_OFFSET, 0);
        assert_eq!(buffer::LEN_OFFSET, 4);
        assert_eq!(buffer::WIDTH_OFFSET, 8);
        assert_eq!(buffer::HEIGHT_OFFSET, 12);
        assert_eq!(buffer::HEADER_SIZE, 16);
    }
}

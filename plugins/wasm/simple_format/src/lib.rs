//! Simple Format Plugin - WASM Module
//!
//! Exports format codec ABI functions for the Kaleido host.
//!
//! This crate is designed to compile to `wasm32-unknown-unknown`.  On that
//! target it uses `#![no_std]` + a custom bump allocator.  For host-side
//! `cargo check` / `cargo test` it falls back to `std` so the crate is
//! type-checkable without the WASM target installed.

#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

// ── Allocator (wasm32 only) ─────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_alloc {
    use alloc::alloc::{GlobalAlloc, Layout};
    use core::sync::atomic::{AtomicUsize, Ordering};

    const HEAP_START: usize = 64 * 1024;
    const HEAP_SIZE: usize = 64 * 1024;

    struct BumpAllocator {
        offset: AtomicUsize,
    }

    unsafe impl GlobalAlloc for BumpAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let align = layout.align();
            let size = layout.size();
            let current = self.offset.load(Ordering::SeqCst);
            let aligned = (current + align - 1) & !(align - 1);
            let new_offset = aligned + size;
            if new_offset > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            self.offset
                .compare_exchange(current, new_offset, Ordering::SeqCst, Ordering::SeqCst)
                .ok();
            (HEAP_START + aligned) as *mut u8
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }

    #[global_allocator]
    static ALLOCATOR: BumpAllocator = BumpAllocator {
        offset: AtomicUsize::new(0),
    };
}

/// Panic handler for WASM (no_std).  Not used on host.
#[cfg(all(target_arch = "wasm32", not(test)))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ── Memory Management ABI ──────────────────────────────────────────────

/// Allocate memory in WASM.
///
/// On `wasm32-unknown-unknown` this delegates to the bump allocator.
/// On the host target it uses the system allocator via `Vec`.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: i32) -> i32 {
    use alloc::alloc::Layout;
    if size <= 0 {
        return 0;
    }
    let layout = match Layout::from_size_align(size as usize, 1) {
        Ok(l) => l,
        Err(_) => return 0,
    };
    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() {
        0
    } else {
        ptr as i32
    }
}

/// Allocate memory (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as i32
}

/// Free memory in WASM.
///
/// On WASM this is a no-op (bump allocator).  On the host it frees the
/// memory allocated by `alloc`.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn free(_ptr: i32, _size: i32) {
    // Bump allocator — no individual free.
}

/// Free memory (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn free(ptr: i32, size: i32) {
    if ptr == 0 || size <= 0 {
        return;
    }
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize);
    }
}

// ── Format Codec ────────────────────────────────────────────────────────

/// Decode an image file.
///
/// Returns: i64 (high = ptr, low = len), or -1 on error.
///
/// This stub decodes any file as a single red RGBA pixel (1×1, 4 bytes).
#[unsafe(no_mangle)]
pub extern "C" fn format_decode(_path_ptr: i32, _path_len: i32) -> i64 {
    let ptr = alloc(4);
    if ptr == 0 {
        return -1;
    }

    // Write a single red pixel (R=255, G=0, B=0, A=255).
    unsafe {
        core::ptr::write_volatile(ptr as *mut u8, 255); // R
        core::ptr::write_volatile((ptr as *mut u8).add(1), 0); // G
        core::ptr::write_volatile((ptr as *mut u8).add(2), 0); // B
        core::ptr::write_volatile((ptr as *mut u8).add(3), 255); // A
    }

    // Return handle: high=ptr, low=len(4)
    ((ptr as i64) << 32) | 4
}

/// Encode image data to file.
///
/// Returns: 0 = success, -1 = error.
#[unsafe(no_mangle)]
pub extern "C" fn format_encode(
    _path_ptr: i32,
    _path_len: i32,
    _data_ptr: i32,
    _data_len: i32,
    _width: i32,
    _height: i32,
) -> i32 {
    0 // Success — stub always succeeds
}

// ── Plugin Lifecycle ────────────────────────────────────────────────────

/// Plugin initialization.
#[unsafe(no_mangle)]
pub extern "C" fn plugin_init() {}

// ── Manifest & Metadata (wasm32 only; host returns 0) ───────────────────

/// Manifest JSON describing this plugin.
#[cfg(target_arch = "wasm32")]
static MANIFEST: &[u8] = br#"{
    "name": "simple_format",
    "version": "0.1.0",
    "description": "Simple format codec stub for Kaleido",
    "author": "Kaleido Team",
    "kind": "wasm",
    "formats": [
        { "name": "simple", "extensions": ["simple"], "can_read": true, "can_write": true }
    ]
}"#;

/// Get plugin manifest JSON pointer.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_manifest_json() -> i32 {
    MANIFEST.as_ptr() as i32
}

/// Get plugin manifest JSON pointer (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_manifest_json() -> i32 {
    0
}

/// Get manifest JSON length.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_manifest_json_len() -> i32 {
    MANIFEST.len() as i32
}

/// Get manifest JSON length (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_manifest_json_len() -> i32 {
    0
}

/// Format name.
#[cfg(target_arch = "wasm32")]
static FORMAT_NAME: &[u8] = b"simple";

/// Get format name pointer.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_name() -> i32 {
    FORMAT_NAME.as_ptr() as i32
}

/// Get format name pointer (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_name() -> i32 {
    0
}

/// Get format name length.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_name_len() -> i32 {
    FORMAT_NAME.len() as i32
}

/// Get format name length (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_name_len() -> i32 {
    0
}

/// Supported extensions.
#[cfg(target_arch = "wasm32")]
static FORMAT_EXTS: &[u8] = b"simple";

/// Get supported extensions pointer.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_exts() -> i32 {
    FORMAT_EXTS.as_ptr() as i32
}

/// Get supported extensions pointer (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_exts() -> i32 {
    0
}

/// Get supported extensions length.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_exts_len() -> i32 {
    FORMAT_EXTS.len() as i32
}

/// Get supported extensions length (host stub).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn format_get_exts_len() -> i32 {
    0
}

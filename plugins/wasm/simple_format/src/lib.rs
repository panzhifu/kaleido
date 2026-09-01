//! Simple Format Plugin - WASM Module
//!
//! Exports format codec ABI functions for the Kaleido host.

#![no_std]

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

/// Simple bump allocator for WASM
struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        let start = 64 * 1024;
        start as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

/// Panic handler for WASM
#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ── Memory Management ────────────────────────────────────────────────────

/// Allocate memory in WASM (stub - returns fixed address)
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: i32) -> i32 {
    // Simple bump allocator starting at 64KB
    let _ = size;
    65536 // Return fixed address
}

/// Free memory in WASM (stub - no-op)
#[unsafe(no_mangle)]
pub extern "C" fn free(_ptr: i32, _size: i32) {
    // No-op
}

// ── Format Codec ─────────────────────────────────────────────────────────

/// Decode an image file
/// Returns: i64 (high = ptr, low = len)
#[unsafe(no_mangle)]
pub extern "C" fn format_decode(_path_ptr: i32, _path_len: i32) -> i64 {
    // Allocate buffer for decoded data (4 bytes for RGBA pixel)
    let ptr = alloc(16);

    if ptr == 0 {
        return -1;
    }

    // Write simple RGBA pixel data (red pixel: 255, 0, 0, 255)
    unsafe {
        ptr::write_volatile((ptr as *mut u8).add(0), 255); // R
        ptr::write_volatile((ptr as *mut u8).add(1), 0);   // G
        ptr::write_volatile((ptr as *mut u8).add(2), 0);   // B
        ptr::write_volatile((ptr as *mut u8).add(3), 255); // A
    }

    // Return handle: high=ptr, low=len(4)
    ((ptr as i64) << 32) | (4 as i64)
}

/// Encode image data to file
/// Returns: i32 (0=success, -1=error)
#[unsafe(no_mangle)]
pub extern "C" fn format_encode(
    _path_ptr: i32,
    _path_len: i32,
    _data_ptr: i32,
    _data_len: i32,
    _width: i32,
    _height: i32,
) -> i32 {
    0 // Success
}

// ── Plugin Lifecycle ─────────────────────────────────────────────────────

/// Plugin initialization
#[unsafe(no_mangle)]
pub extern "C" fn plugin_init() {}

/// Get plugin manifest JSON
#[unsafe(no_mangle)]
pub extern "C" fn plugin_manifest_json() -> i32 {
    0
}

/// Get format name
#[unsafe(no_mangle)]
pub extern "C" fn format_get_name() -> i32 {
    0
}

/// Get supported extensions
#[unsafe(no_mangle)]
pub extern "C" fn format_get_exts() -> i32 {
    0
}

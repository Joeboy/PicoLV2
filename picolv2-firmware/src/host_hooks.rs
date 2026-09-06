//! Stable host functions resolved into plugin binaries at relocation time (see
//! `plugin_host.rs`'s `SyntheticModule`). These are the small C ABI used by
//! `plugins/pico-alloc.c`; C++ runtime compatibility functions belong in the
//! plugin build and are not part of the host ABI.

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;

use defmt::info;

use crate::HEAP;

/// Backs plugin `malloc`/`calloc`/`realloc`/`free` with the same tracked heap
/// used for loading plugin ELF images.
#[unsafe(no_mangle)]
extern "C" fn picolv2_alloc(size: usize, align: usize) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(size, align.max(1)) else {
        return core::ptr::null_mut();
    };
    unsafe { HEAP.alloc(layout) }
}

#[unsafe(no_mangle)]
extern "C" fn picolv2_dealloc(pointer: *mut u8, size: usize, align: usize) {
    if pointer.is_null() {
        return;
    }
    let Ok(layout) = Layout::from_size_align(size, align.max(1)) else {
        return;
    };
    unsafe { HEAP.dealloc(pointer, layout) };
}

/// Backs plugin `printf`/`puts` (see `_write` in `plugins/pico-alloc.c`) by
/// forwarding raw stdout bytes into the defmt/RTT log.
#[unsafe(no_mangle)]
extern "C" fn picolv2_log_write(pointer: *const u8, len: i32) {
    if pointer.is_null() || len <= 0 {
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts(pointer, len as usize) };
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    match core::str::from_utf8(bytes) {
        Ok(text) => info!("plugin: {}", text),
        Err(_) => info!("plugin: {=[u8]}", bytes),
    }
}

// Keep this list limited to host services that require firmware state or I/O.
// Plugin-local runtime shims should not become accidental ABI dependencies.
pub const HOST_SYMBOLS: [(&str, *const ()); 3] = [
    ("picolv2_alloc", picolv2_alloc as *const ()),
    ("picolv2_dealloc", picolv2_dealloc as *const ()),
    ("picolv2_log_write", picolv2_log_write as *const ()),
];

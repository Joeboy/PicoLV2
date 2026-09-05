//! Host functions resolved into plugin binaries at relocation time (see
//! `plugin_host.rs`'s `SyntheticModule`), backing symbols left undefined by
//! `plugins/pico-alloc.c`, plus a minimal C++ ABI (`operator new`/`delete`,
//! `__cxa_guard_*`, `atexit`) for plugins built from C++ (e.g. the ams-lv2
//! port), which is linked with `-nostdlib` and no libstdc++/libsupc++.

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

pub const HOST_SYMBOLS: [(&str, *const ()); 10] = [
    ("picolv2_alloc", picolv2_alloc as *const ()),
    ("picolv2_dealloc", picolv2_dealloc as *const ()),
    ("picolv2_log_write", picolv2_log_write as *const ()),
    ("_Znwj", cxx_operator_new as *const ()),
    ("_ZdlPv", cxx_operator_delete as *const ()),
    ("__cxa_guard_acquire", __cxa_guard_acquire as *const ()),
    ("__cxa_guard_release", __cxa_guard_release as *const ()),
    ("atexit", atexit as *const ()),
    ("_ZSt20__throw_length_errorPKc", cxx_throw_length_error as *const ()),
    ("_fini", fini as *const ()),
];

// C++'s unsized `operator delete` doesn't carry the original allocation size,
// so (like `pico-alloc.c`'s malloc/free) we stash it in a header before the
// returned pointer.
#[repr(C)]
struct CxxAllocHeader {
    size: usize,
}
const CXX_ALLOC_ALIGN: usize = align_of::<CxxAllocHeader>();

#[unsafe(export_name = "_Znwj")]
extern "C" fn cxx_operator_new(size: usize) -> *mut u8 {
    let total = size.saturating_add(size_of::<CxxAllocHeader>());
    let Ok(layout) = Layout::from_size_align(total, CXX_ALLOC_ALIGN) else {
        return core::ptr::null_mut();
    };
    let raw = unsafe { HEAP.alloc(layout) };
    if raw.is_null() {
        return core::ptr::null_mut();
    }
    unsafe {
        (raw as *mut CxxAllocHeader).write(CxxAllocHeader { size: total });
        raw.add(size_of::<CxxAllocHeader>())
    }
}

#[unsafe(export_name = "_ZdlPv")]
extern "C" fn cxx_operator_delete(pointer: *mut u8) {
    if pointer.is_null() {
        return;
    }
    unsafe {
        let raw = pointer.sub(size_of::<CxxAllocHeader>());
        let size = (raw as *const CxxAllocHeader).read().size;
        let Ok(layout) = Layout::from_size_align(size, CXX_ALLOC_ALIGN) else {
            return;
        };
        HEAP.dealloc(raw, layout);
    }
}

// Single-core, no-threads guard for C++ function-local statics (Itanium C++
// ABI). The low byte is 1 once initialization has completed.
#[unsafe(no_mangle)]
extern "C" fn __cxa_guard_acquire(guard: *mut u8) -> i32 {
    if unsafe { *guard } != 0 { 0 } else { 1 }
}

#[unsafe(no_mangle)]
extern "C" fn __cxa_guard_release(guard: *mut u8) {
    unsafe { *guard = 1 };
}

// Plugins never "exit", so destructors registered via `atexit` (e.g. for
// static objects built with `-fno-use-cxa-atexit`) would never run anyway.
#[unsafe(no_mangle)]
extern "C" fn atexit(_callback: *const c_void) -> i32 {
    0
}

// newlib's `__libc_fini_array` (pulled in via libc.a) calls this at program
// exit; plugins never exit, so it's never actually invoked.
#[unsafe(export_name = "_fini")]
extern "C" fn fini() {}

// libstdc++ containers (e.g. std::vector) call this for defensive length
// checks compiled in regardless of `-fno-exceptions`; plugins are built
// without exception support, so there's no handler to unwind to.
#[unsafe(export_name = "_ZSt20__throw_length_errorPKc")]
extern "C" fn cxx_throw_length_error(_message: *const core::ffi::c_char) -> ! {
    panic!("C++ plugin called std::__throw_length_error");
}

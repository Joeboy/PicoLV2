#![no_std]

extern crate alloc;

mod abi;
mod synth;

use alloc::boxed::Box;
use abi::{ATOM_SEQUENCE_URI, Lv2Descriptor, Lv2Feature, Lv2Handle, Lv2UridMap, MIDI_EVENT_URI, PLUGIN_URI, URID_MAP_URI};
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_void, CStr};
use synth::StringSynth;

struct CAllocator;

unsafe impl GlobalAlloc for CAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { malloc(layout.size()).cast() }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { free(ptr.cast()) }
    }
}

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
}

#[global_allocator]
static ALLOCATOR: CAllocator = CAllocator;

extern "C" fn instantiate(_: *const Lv2Descriptor, sample_rate: f64, _: *const c_char, features: *const *const Lv2Feature) -> Lv2Handle {
    if features.is_null() { return core::ptr::null_mut(); }
    let mut map: *const Lv2UridMap = core::ptr::null();
    let mut feature = features;
    while !unsafe { (*feature).is_null() } {
        let item = unsafe { &**feature };
        if !item.uri.is_null() && unsafe { CStr::from_ptr(item.uri).to_bytes_with_nul() == URID_MAP_URI } { map = item.data.cast(); break; }
        feature = unsafe { feature.add(1) };
    }
    if map.is_null() { return core::ptr::null_mut(); }
    let map = unsafe { &*map };
    let sequence = (map.map)(map.handle, ATOM_SEQUENCE_URI.as_ptr().cast());
    let midi = (map.map)(map.handle, MIDI_EVENT_URI.as_ptr().cast());
    if sequence == 0 || midi == 0 { return core::ptr::null_mut(); }
    let mut string_synth = Box::new(StringSynth::new());
    string_synth.initialise(sample_rate as f32, sequence, midi);
    Box::into_raw(string_synth) as Lv2Handle
}
extern "C" fn connect_port(handle: Lv2Handle, port: u32, data: *mut c_void) { if let Some(plugin) = unsafe { handle.cast::<StringSynth>().as_mut() } { plugin.connect_port(port, data); } }
extern "C" fn activate(handle: Lv2Handle) { if let Some(plugin) = unsafe { handle.cast::<StringSynth>().as_mut() } { plugin.activate(); } }
extern "C" fn run(handle: Lv2Handle, count: u32) { if let Some(plugin) = unsafe { handle.cast::<StringSynth>().as_mut() } { unsafe { plugin.run(count); } } }
extern "C" fn noop(_: Lv2Handle) {}
extern "C" fn cleanup(handle: Lv2Handle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle.cast::<StringSynth>())); }
    }
}
extern "C" fn extension_data(_: *const c_char) -> *const c_void { core::ptr::null() }

static DESCRIPTOR: Lv2Descriptor = Lv2Descriptor { uri: PLUGIN_URI.as_ptr().cast(), instantiate, connect_port, activate, run, deactivate: noop, cleanup, extension_data };
#[unsafe(no_mangle)] pub extern "C" fn lv2_descriptor(index: u32) -> *const Lv2Descriptor { if index == 0 { &raw const DESCRIPTOR } else { core::ptr::null() } }
#[panic_handler] fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop { core::hint::spin_loop(); } }

#![no_std]
#![no_main]

use core::ffi::{CStr, c_char, c_void};

use defmt::*;
use elf_loader::{
    Loader, Relocator,
    image::{SyntheticModule, SyntheticSymbol},
    input::ElfBinary,
};
use embassy_executor::Executor;
use embedded_alloc::LlffHeap as Heap;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

// The example plugins, built by their own Makefiles, embedded directly into
// this binary (dynamic loading is still done by `elf_loader` at runtime; this
// is just how the plugin bytes get onto the device for now).
static PLUGIN: &[u8] = include_bytes!("../../example-plugin/build/plugin.so");
static LV2_PLUGIN: &[u8] = include_bytes!("../../example-lv2/build/pico/plugin.so");

// Mirrors the C `LV2_Descriptor` in example-lv2/src/plugin.c field-for-field.
#[repr(C)]
struct Lv2Descriptor {
    uri: *const c_char,
    instantiate: extern "C" fn(
        *const Lv2Descriptor,
        f64,
        *const c_char,
        *const *const c_void,
    ) -> *mut c_void,
    connect_port: extern "C" fn(*mut c_void, u32, *mut c_void),
    activate: extern "C" fn(*mut c_void),
    run: extern "C" fn(*mut c_void, u32),
    deactivate: extern "C" fn(*mut c_void),
    cleanup: extern "C" fn(*mut c_void),
    extension_data: extern "C" fn(*const c_char) -> *const c_void,
}

#[global_allocator]
static HEAP: Heap = Heap::empty();

const HEAP_SIZE: usize = 128 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

// Provided to the plugin as a host callback (see `host_double` in
// example-plugin/src/plugin.c), resolved into the plugin's PLT at load time.
extern "C" fn host_double(x: i32) -> i32 {
    x * 2
}

#[embassy_executor::task]
async fn run_task() {
    run_example_plugin_tests();
    run_example_lv2_tests();

    loop {
        embassy_time::Timer::after_secs(1).await;
    }
}

fn run_example_plugin_tests() {
    let raw = Loader::new()
        .run()
        .load_dylib(ElfBinary::new("plugin.so", PLUGIN))
        .expect("failed to load plugin");

    let host = SyntheticModule::new(
        "__host",
        [SyntheticSymbol::function(
            "host_double",
            host_double as *const (),
        )],
    );

    let lib = Relocator::new()
        .run(raw)
        .modules([host])
        .relocate()
        .expect("failed to relocate plugin");

    let return_23 = unsafe {
        lib.get::<extern "C" fn() -> i32>("return_23")
            .expect("symbol `return_23` not found")
    };
    let result = return_23();
    debug!("return_23() = {}", result);

    // Passing parameters from host to plugin, and getting a value back.
    let add = unsafe {
        lib.get::<extern "C" fn(i32, i32) -> i32>("add")
            .expect("symbol `add` not found")
    };
    debug!("add(2, 3) = {}", add(2, 3));

    // Calls another (non-exported) function within the plugin, passing a
    // parameter to it and returning its result back to us.
    let add_one = unsafe {
        lib.get::<extern "C" fn(i32) -> i32>("add_one")
            .expect("symbol `add_one` not found")
    };
    debug!("add_one(41) = {}", add_one(41));

    // Passing a pointer/buffer the host owns for the plugin to read.
    let sum_buffer = unsafe {
        lib.get::<extern "C" fn(*const i32, i32) -> i32>("sum_buffer")
            .expect("symbol `sum_buffer` not found")
    };
    let buf: [i32; 4] = [10, 20, 30, 40];
    debug!("sum_buffer(&buf, 4) = {}", sum_buffer(buf.as_ptr(), 4));

    // The plugin calls back into `host_double`, provided above.
    let double_via_host = unsafe {
        lib.get::<extern "C" fn(i32) -> i32>("double_via_host")
            .expect("symbol `double_via_host` not found")
    };
    debug!("double_via_host(9) = {}", double_via_host(9));
}

fn run_example_lv2_tests() {
    let raw = Loader::new()
        .run()
        .load_dylib(ElfBinary::new("lv2-plugin.so", LV2_PLUGIN))
        .expect("failed to load lv2 plugin");
    let lib = Relocator::new()
        .run(raw)
        .relocate()
        .expect("failed to relocate lv2 plugin");

    let lv2_descriptor = unsafe {
        lib.get::<extern "C" fn(u32) -> *const Lv2Descriptor>("lv2_descriptor")
            .expect("symbol `lv2_descriptor` not found")
    };

    let descriptor = unsafe { &*lv2_descriptor(0) };
    let uri = unsafe { CStr::from_ptr(descriptor.uri) };
    debug!("lv2_descriptor(0).uri = {}", uri.to_str().unwrap_or("<invalid utf8>"));

    let handle = (descriptor.instantiate)(descriptor, 48000.0, core::ptr::null(), core::ptr::null());

    let frequency: f32 = 8000.0;
    let mut output: [f32; 16] = [0.0; 16];

    (descriptor.connect_port)(handle, 0, (&frequency as *const f32) as *mut c_void);
    (descriptor.connect_port)(handle, 1, (output.as_mut_ptr()) as *mut c_void);

    (descriptor.activate)(handle);
    (descriptor.run)(handle, output.len() as u32);
    (descriptor.deactivate)(handle);
    (descriptor.cleanup)(handle);

    debug!("lv2 square synth output (freq={}) = {}", frequency, output);
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let _p = embassy_rp::init(Default::default());
    unsafe {
        HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
    }
    info!("pico-loader starting");

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| spawner.spawn(unwrap!(run_task())))
}

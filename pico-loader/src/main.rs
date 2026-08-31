#![no_std]
#![no_main]

use defmt::*;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use embassy_executor::Executor;
use embedded_alloc::LlffHeap as Heap;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

// The example plugin, built by `example-plugin/Makefile`, embedded directly
// into this binary (dynamic loading is still done by `elf_loader` at runtime;
// this is just how the plugin bytes get onto the device for now).
static PLUGIN: &[u8] = include_bytes!("../../example-plugin/build/plugin.so");

#[global_allocator]
static HEAP: Heap = Heap::empty();

const HEAP_SIZE: usize = 128 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn run_task() {
    let raw = Loader::new()
        .run()
        .load_dylib(ElfBinary::new("plugin.so", PLUGIN))
        .expect("failed to load plugin");
    let lib = Relocator::new()
        .run(raw)
        .relocate()
        .expect("failed to relocate plugin");

    let return_23 = unsafe {
        lib.get::<extern "C" fn() -> i32>("return_23")
            .expect("symbol `return_23` not found")
    };
    let result = return_23();
    debug!("return_23() = {}", result);

    loop {
        embassy_time::Timer::after_secs(1).await;
    }
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

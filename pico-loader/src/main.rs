#![no_std]
#![no_main]

mod audio_out;
mod i2s_ping_pong;
mod lv2;
mod midi;
mod usb_midi_in;

use core::ffi::{CStr, c_void};

use audio_out::audio_task;
use defmt::*;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use embassy_executor::Executor;
use embedded_alloc::LlffHeap as Heap;
use lv2::Lv2Descriptor;
use midi::MIDI_QUEUE;
use heapless::spsc::Queue;
use static_cell::StaticCell;
use usb_midi_in::usb_midi_task;
use {defmt_rtt as _, panic_probe as _};

static LV2_PLUGIN: &[u8] = include_bytes!("../../example-lv2/build/pico/plugin.so");

#[global_allocator]
static HEAP: Heap = Heap::empty();

const HEAP_SIZE: usize = 256 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn run_task() {
    // run_example_lv2_tests();

    loop {
        embassy_time::Timer::after_secs(1).await;
    }
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

    let mut output: [f32; 16] = [0.0; 16];

    (descriptor.connect_port)(handle, 0, (output.as_mut_ptr()) as *mut c_void);

    (descriptor.activate)(handle);
    (descriptor.run)(handle, output.len() as u32);
    (descriptor.deactivate)(handle);
    (descriptor.cleanup)(handle);

    debug!("lv2 square synth output (no MIDI note) = {}", output);
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());
    unsafe {
        HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
    }
    info!("pico-loader starting");

    let midi_queue = MIDI_QUEUE.init(Queue::new());
    let (midi_producer, midi_consumer) = midi_queue.split();

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(unwrap!(run_task()));
        spawner.spawn(unwrap!(audio_task(
            p.PIO0,
            p.DMA_CH0,
            p.DMA_CH1,
            p.PIN_18,
            p.PIN_19,
            p.PIN_20,
            midi_consumer,
        )));
        spawner.spawn(unwrap!(usb_midi_task(p.USB, midi_producer)));
    })
}

#![no_std]
#![no_main]

mod audio_buffer;
mod audio_out;
mod i2s_ping_pong;
mod lv2;
mod midi;
mod plugin_host;
mod usb_midi_in;

use audio_out::audio_task;
use audio_buffer::AUDIO_QUEUE;
use defmt::*;
use embassy_executor::Executor;
use embassy_rp::multicore::{Stack, spawn_core1};
use embedded_alloc::LlffHeap as Heap;
use heapless::spsc::Queue;
use midi::MIDI_QUEUE;
use plugin_host::plugin_host_task;
use static_cell::StaticCell;
use usb_midi_in::usb_midi_task;
use {defmt_rtt as _, panic_probe as _};

#[global_allocator]
static HEAP: Heap = Heap::empty();

const HEAP_SIZE: usize = 256 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static mut CORE1_STACK: Stack<16384> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());
    unsafe {
        HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
    }
    info!("pico-loader starting");

    let midi_queue = MIDI_QUEUE.init(Queue::new());
    let (midi_producer, midi_consumer) = midi_queue.split();
    let audio_queue = AUDIO_QUEUE.init(Queue::new());
    let (audio_producer, audio_consumer) = audio_queue.split();

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(unwrap!(plugin_host_task(midi_consumer, audio_producer)));
            });
        },
    );

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        spawner.spawn(unwrap!(audio_task(
            p.PIO0,
            p.DMA_CH0,
            p.DMA_CH1,
            p.PIN_18,
            p.PIN_19,
            p.PIN_20,
            audio_consumer,
        )));
        spawner.spawn(unwrap!(usb_midi_task(p.USB, midi_producer)));
    })
}

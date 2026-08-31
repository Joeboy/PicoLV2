#![no_std]
#![no_main]

mod audio_out;
mod i2s_ping_pong;
mod lv2;
mod midi;
mod usb_midi_in;

use audio_out::audio_task;
use defmt::*;
use embassy_executor::Executor;
use embedded_alloc::LlffHeap as Heap;
use heapless::spsc::Queue;
use midi::MIDI_QUEUE;
use static_cell::StaticCell;
use usb_midi_in::usb_midi_task;
use {defmt_rtt as _, panic_probe as _};

#[global_allocator]
static HEAP: Heap = Heap::empty();

const HEAP_SIZE: usize = 256 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

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

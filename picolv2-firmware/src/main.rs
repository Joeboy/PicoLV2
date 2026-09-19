#![no_std]
#![no_main]

extern crate alloc;

mod audio;
mod diagnostics;
mod hardware;
mod midi;
mod plugin_host;

use audio::{AUDIO_BLOCK_COUNT, FREE_AUDIO_BLOCKS, READY_AUDIO_BLOCKS};
use diagnostics::{SystemHeap, diag_info};
use embassy_executor::Executor;
use embassy_rp::multicore::{Stack, spawn_core1};
use hardware::{audio_task, usb_midi_task};
use heapless::spsc::Queue;
use midi::MIDI_QUEUE;
use panic_probe as _;
use plugin_host::plugin_host_task;
use static_cell::StaticCell;

#[global_allocator]
static HEAP: SystemHeap = SystemHeap::empty();

const HEAP_SIZE: usize = 384 * 1024;
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
    diag_info!("picolv2-firmware starting");
    log_heap("after init");

    let midi_queue = MIDI_QUEUE.init(Queue::new());
    let (midi_producer, midi_consumer) = midi_queue.split();
    let free_audio_blocks = FREE_AUDIO_BLOCKS.init(Queue::new());
    for index in 0..AUDIO_BLOCK_COUNT {
        free_audio_blocks
            .enqueue(index as u8)
            .expect("free audio block queue too small");
    }
    let (free_producer, free_consumer) = free_audio_blocks.split();
    let ready_audio_blocks = READY_AUDIO_BLOCKS.init(Queue::new());
    let (ready_producer, ready_consumer) = ready_audio_blocks.split();

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(
                    plugin_host_task(midi_consumer, free_consumer, ready_producer)
                        .expect("plugin host task already spawned"),
                );
            });
        },
    );

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        spawner.spawn(
            audio_task(
                p.PIO0,
                p.DMA_CH0,
                p.DMA_CH1,
                p.PIN_18,
                p.PIN_19,
                p.PIN_20,
                ready_consumer,
                free_producer,
            )
            .expect("audio task already spawned"),
        );
        spawner.spawn(usb_midi_task(p.USB, midi_producer).expect("USB MIDI task already spawned"));
    })
}

pub fn log_heap(stage: &'static str) {
    HEAP.log(stage);
}

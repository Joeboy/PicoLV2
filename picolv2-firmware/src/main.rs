#![no_std]
#![no_main]

mod audio_buffer;
mod audio_out;
mod i2s_ping_pong;
mod lv2;
mod midi;
mod plugin_host;
mod usb_midi_in;

use audio_buffer::{AUDIO_BLOCK_COUNT, FREE_AUDIO_BLOCKS, READY_AUDIO_BLOCKS};
use audio_out::audio_task;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use defmt::*;
use embassy_executor::Executor;
use embassy_rp::multicore::{Stack, spawn_core1};
use embedded_alloc::TlsfHeap as Heap;
use heapless::spsc::Queue;
use midi::MIDI_QUEUE;
use plugin_host::plugin_host_task;
use static_cell::StaticCell;
use usb_midi_in::usb_midi_task;
use {defmt_rtt as _, panic_probe as _};

struct CountingHeap {
    heap: Heap,
    current: AtomicUsize,
    peak: AtomicUsize,
    allocations: AtomicUsize,
    last_request: AtomicUsize,
    failures: AtomicUsize,
    failed_request: AtomicUsize,
}

unsafe impl GlobalAlloc for CountingHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.last_request.store(layout.size(), Ordering::Relaxed);
        let pointer = unsafe { self.heap.alloc(layout) };
        if !pointer.is_null() {
            let current = self.current.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.peak.fetch_max(current, Ordering::Relaxed);
        } else {
            self.failures.fetch_add(1, Ordering::Relaxed);
            self.failed_request.store(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { self.heap.dealloc(pointer, layout) };
        self.current.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static HEAP: CountingHeap = CountingHeap {
    heap: Heap::empty(),
    current: AtomicUsize::new(0),
    peak: AtomicUsize::new(0),
    allocations: AtomicUsize::new(0),
    last_request: AtomicUsize::new(0),
    failures: AtomicUsize::new(0),
    failed_request: AtomicUsize::new(0),
};

const HEAP_SIZE: usize = 384 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

static mut CORE1_STACK: Stack<16384> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());
    unsafe {
        HEAP.heap
            .init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
    }
    info!("picolv2-firmware starting");
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
                spawner.spawn(unwrap!(plugin_host_task(
                    midi_consumer,
                    free_consumer,
                    ready_producer,
                )));
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
            ready_consumer,
            free_producer,
        )));
        spawner.spawn(unwrap!(usb_midi_task(p.USB, midi_producer)));
    })
}

pub fn log_heap(stage: &'static str) {
    info!(
        "heap {} current={} peak={} allocations={} last_request={} failures={} failed_request={}",
        stage,
        HEAP.current.load(Ordering::Relaxed),
        HEAP.peak.load(Ordering::Relaxed),
        HEAP.allocations.load(Ordering::Relaxed),
        HEAP.last_request.load(Ordering::Relaxed),
        HEAP.failures.load(Ordering::Relaxed),
        HEAP.failed_request.load(Ordering::Relaxed),
    );
}

/// Backs plugin `malloc`/`calloc`/`realloc`/`free` (see `plugins/pico-alloc.c`)
/// with the same tracked heap used for loading plugin ELF images, resolved at
/// plugin-relocation time via a synthetic module (see `plugin_host.rs`).
#[unsafe(no_mangle)]
pub extern "C" fn picolv2_alloc(size: usize, align: usize) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(size, align.max(1)) else {
        return core::ptr::null_mut();
    };
    unsafe { HEAP.alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn picolv2_dealloc(pointer: *mut u8, size: usize, align: usize) {
    if pointer.is_null() {
        return;
    }
    let Ok(layout) = Layout::from_size_align(size, align.max(1)) else {
        return;
    };
    unsafe { HEAP.dealloc(pointer, layout) };
}

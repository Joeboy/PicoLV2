use core::ffi::c_void;
use core::ops::ControlFlow;

use defmt::info;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use embassy_rp::Peri;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, PIN_18, PIN_19, PIN_20, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use {defmt_rtt as _, panic_probe as _};

use crate::i2s_ping_pong::{PioI2sOut, PioI2sOutProgram};
use crate::lv2::Lv2Descriptor;
use crate::midi::{MIDI_BLOCK_CAPACITY, MIDI_QUEUE_SIZE, MidiEvent, MidiEventBlock};
use heapless::spsc::Consumer;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

// Same plugin as the one used for the printed tests in main.rs, but loaded
// as its own independent instance dedicated to continuous audio output.
static LV2_PLUGIN: &[u8] = include_bytes!("../../example-lv2/build/pico/plugin.so");

const SAMPLE_RATE: u32 = 48_000;
const BIT_DEPTH: u32 = 16;
const BUFFER_SIZE: usize = 512;

// Connected to the plugin's audio output port; must have a stable address
// for as long as the plugin may write into it, so it can't just live inside
// a value that gets moved (e.g. into an async task's captured state).
static mut SCRATCH: [f32; BUFFER_SIZE] = [0.0; BUFFER_SIZE];
static mut MIDI_EVENTS: [MidiEvent; MIDI_BLOCK_CAPACITY] =
    [MidiEvent::EMPTY; MIDI_BLOCK_CAPACITY];
static mut MIDI_BLOCK: MidiEventBlock = MidiEventBlock {
    events: core::ptr::null(),
    event_count: 0,
};

// Pack left and right 16-bit samples into a single u32, as that's what the I2S DMA expects.
#[inline]
fn pack_lr_16(l: i16, r: i16) -> u32 {
    ((l as u32 as u16 as u32) << 16) | ((r as u16) as u32)
}

/// Drives the loaded LV2 synth plugin's `run()` once per DMA buffer swap,
/// converting its float samples into packed 16-bit stereo I2S words.
struct Lv2Synth {
    descriptor: &'static Lv2Descriptor,
    handle: *mut c_void,
    midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>,
}

impl Lv2Synth {
    fn process(&mut self, buf: &mut [u32]) -> ControlFlow<()> {
        let scratch = unsafe { &mut *core::ptr::addr_of_mut!(SCRATCH) };
        let midi_events = unsafe { &mut *core::ptr::addr_of_mut!(MIDI_EVENTS) };
        let mut event_count = 0;
        while event_count < midi_events.len() {
            let Some(event) = self.midi_consumer.dequeue() else {
                break;
            };
            midi_events[event_count] = event;
            event_count += 1;
        }
        unsafe {
            MIDI_BLOCK.events = midi_events.as_ptr();
            MIDI_BLOCK.event_count = event_count as u32;
        }

        (self.descriptor.run)(self.handle, buf.len() as u32);
        for (word, sample) in buf.iter_mut().zip(scratch.iter()) {
            let pcm = (*sample * i16::MAX as f32) as i16;
            *word = pack_lr_16(pcm, pcm);
        }
        ControlFlow::Continue(())
    }
}

#[embassy_executor::task]
pub async fn audio_task(
    pio0: Peri<'static, PIO0>,
    dma_ch0: Peri<'static, DMA_CH0>,
    dma_ch1: Peri<'static, DMA_CH1>,
    pin18: Peri<'static, PIN_18>,
    pin19: Peri<'static, PIN_19>,
    pin20: Peri<'static, PIN_20>,
    midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>,
) {
    info!("Starting I2S audio output task (LV2 square synth)");

    let raw = Loader::new()
        .run()
        .load_dylib(ElfBinary::new("lv2-plugin.so", LV2_PLUGIN))
        .expect("failed to load lv2 plugin for audio output");
    let lib = Relocator::new()
        .run(raw)
        .relocate()
        .expect("failed to relocate lv2 plugin for audio output");

    let lv2_descriptor = unsafe {
        lib.get::<extern "C" fn(u32) -> *const Lv2Descriptor>("lv2_descriptor")
            .expect("symbol `lv2_descriptor` not found")
    };
    let descriptor: &'static Lv2Descriptor = unsafe { &*lv2_descriptor(0) };

    // Never released: the loaded image must stay resident in the
    // allocator-backed heap for as long as we keep calling into it, which is
    // for the rest of the program's life.
    core::mem::forget(lib);

    let handle = (descriptor.instantiate)(
        descriptor,
        SAMPLE_RATE as f64,
        core::ptr::null(),
        core::ptr::null(),
    );

    (descriptor.connect_port)(
        handle,
        0,
        core::ptr::addr_of_mut!(SCRATCH) as *mut c_void,
    );
    unsafe {
        MIDI_BLOCK.events = core::ptr::addr_of!(MIDI_EVENTS) as *const MidiEvent;
    }
    (descriptor.connect_port)(
        handle,
        1,
        core::ptr::addr_of_mut!(MIDI_BLOCK) as *mut c_void,
    );
    (descriptor.activate)(handle);

    let mut synth = Lv2Synth {
        descriptor,
        handle,
        midi_consumer,
    };

    let Pio {
        mut common, sm0, ..
    } = Pio::new(pio0, Irqs);

    let bit_clock_pin = pin18;
    let left_right_clock_pin = pin19;
    let data_pin = pin20;

    let program = PioI2sOutProgram::new(&mut common);

    let mut buf_a = [0u32; BUFFER_SIZE];
    let mut buf_b = [0u32; BUFFER_SIZE];

    let mut i2s = PioI2sOut::new(
        &mut common,
        sm0,
        data_pin,
        bit_clock_pin,
        left_right_clock_pin,
        SAMPLE_RATE,
        BIT_DEPTH,
        &program,
    );

    i2s.stream_ping_pong(dma_ch0, dma_ch1, &mut buf_a, &mut buf_b, move |buf| {
        synth.process(buf)
    })
    .await;
}

use core::ffi::c_void;

use defmt::info;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use heapless::spsc::{Consumer, Producer};

use crate::audio_buffer::{AUDIO_QUEUE_SIZE, AudioBlock, BLOCK_SIZE, SAMPLE_RATE};
use crate::lv2::Lv2Descriptor;
use crate::midi::{Lv2MidiSequence, MIDI_QUEUE_SIZE, MidiEvent};

static LV2_PLUGIN: &[u8] = include_bytes!("../../example-lv2/build/pico/plugin.so");

const MIDI_INPUT_PORT: u32 = 0;
const AUDIO_OUTPUT_PORT: u32 = 1;

// Port buffers need stable addresses for the lifetime of the plugin instance.
static mut AUDIO_OUTPUT: AudioBlock = [0.0; BLOCK_SIZE];
static mut MIDI_SEQUENCE: Lv2MidiSequence = Lv2MidiSequence::empty();

/// Owns a loaded LV2 plugin instance and bridges queued MIDI events to its
/// Atom Sequence input port.
pub struct PluginHost {
    descriptor: &'static Lv2Descriptor,
    handle: *mut c_void,
    midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>,
}

impl PluginHost {
    pub fn load(midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>) -> Self {
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
        let descriptor: &'static Lv2Descriptor = unsafe { &*lv2_descriptor(0) };

        // The mapped image must remain resident while its descriptor and code
        // are used. This host keeps the plugin alive for the firmware lifetime.
        core::mem::forget(lib);

        let handle = (descriptor.instantiate)(
            descriptor,
            SAMPLE_RATE as f64,
            core::ptr::null(),
            core::ptr::null(),
        );
        assert!(!handle.is_null(), "failed to instantiate lv2 plugin");

        (descriptor.connect_port)(
            handle,
            MIDI_INPUT_PORT,
            core::ptr::addr_of_mut!(MIDI_SEQUENCE) as *mut c_void,
        );
        (descriptor.connect_port)(
            handle,
            AUDIO_OUTPUT_PORT,
            core::ptr::addr_of_mut!(AUDIO_OUTPUT) as *mut c_void,
        );
        (descriptor.activate)(handle);

        Self {
            descriptor,
            handle,
            midi_consumer,
        }
    }

    pub fn process(&mut self) -> &'static AudioBlock {
        let midi_sequence = unsafe { &mut *core::ptr::addr_of_mut!(MIDI_SEQUENCE) };
        let mut event_count = 0;
        while event_count < midi_sequence.events.len() {
            let Some(event) = self.midi_consumer.dequeue() else {
                break;
            };
            midi_sequence.events[event_count].frame = 0;
            midi_sequence.events[event_count].message = [event.status, event.data1, event.data2];
            event_count += 1;
        }
        midi_sequence.set_event_count(event_count);

        (self.descriptor.run)(self.handle, BLOCK_SIZE as u32);
        unsafe { &*core::ptr::addr_of!(AUDIO_OUTPUT) }
    }
}

#[embassy_executor::task]
pub async fn plugin_host_task(
    midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>,
    mut audio_producer: Producer<'static, AudioBlock, AUDIO_QUEUE_SIZE>,
) -> ! {
    info!("Starting LV2 plugin host task");
    let mut plugin = PluginHost::load(midi_consumer);

    loop {
        let block = *plugin.process();
        while let Err(returned_block) = audio_producer.enqueue(block) {
            let _ = returned_block;
            embassy_futures::yield_now().await;
        }
    }
}

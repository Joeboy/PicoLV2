use core::ffi::{CStr, c_char, c_void};

use defmt::info;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use heapless::spsc::{Consumer, Producer};

use crate::audio_buffer::{
    AUDIO_QUEUE_SIZE, AudioBlockIndex, BLOCK_SIZE, SAMPLE_RATE, block_mut_ptr,
};
use crate::lv2::{
    ATOM_SEQUENCE_URI, ATOM_SEQUENCE_URID, Lv2Descriptor, Lv2Feature, Lv2UridMap,
    MIDI_EVENT_URI, MIDI_EVENT_URID, URID_MAP_URI,
};
use crate::midi::{Lv2MidiSequence, MIDI_QUEUE_SIZE, MidiEvent};

static LV2_PLUGIN: &[u8] = include_bytes!("../../example-lv2/build/pico/plugin.so");

const MIDI_INPUT_PORT: u32 = 0;
const AUDIO_OUTPUT_PORT: u32 = 1;

static mut MIDI_SEQUENCE: Lv2MidiSequence = Lv2MidiSequence::empty();

extern "C" fn map_uri(_handle: *mut c_void, uri: *const c_char) -> u32 {
    if uri.is_null() {
        return 0;
    }

    let uri = unsafe { CStr::from_ptr(uri) }.to_bytes_with_nul();
    if uri == ATOM_SEQUENCE_URI {
        ATOM_SEQUENCE_URID
    } else if uri == MIDI_EVENT_URI {
        MIDI_EVENT_URID
    } else {
        0
    }
}

static mut URID_MAP: Lv2UridMap = Lv2UridMap {
    handle: core::ptr::null_mut(),
    map: map_uri,
};
static mut URID_MAP_FEATURE: Lv2Feature = Lv2Feature {
    uri: URID_MAP_URI.as_ptr() as *const c_char,
    data: core::ptr::addr_of_mut!(URID_MAP) as *mut c_void,
};
static mut FEATURES: [*const Lv2Feature; 2] = [
    core::ptr::addr_of!(URID_MAP_FEATURE),
    core::ptr::null(),
];

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
            core::ptr::addr_of!(FEATURES) as *const *const Lv2Feature,
        );
        assert!(!handle.is_null(), "failed to instantiate lv2 plugin");

        (descriptor.connect_port)(
            handle,
            MIDI_INPUT_PORT,
            core::ptr::addr_of_mut!(MIDI_SEQUENCE) as *mut c_void,
        );
        (descriptor.activate)(handle);

        Self {
            descriptor,
            handle,
            midi_consumer,
        }
    }

    unsafe fn process(&mut self, output: *mut f32) {
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

        (self.descriptor.connect_port)(
            self.handle,
            AUDIO_OUTPUT_PORT,
            output as *mut c_void,
        );
        (self.descriptor.run)(self.handle, BLOCK_SIZE as u32);
    }
}

#[embassy_executor::task]
pub async fn plugin_host_task(
    midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>,
    mut free_consumer: Consumer<'static, AudioBlockIndex, AUDIO_QUEUE_SIZE>,
    mut ready_producer: Producer<'static, AudioBlockIndex, AUDIO_QUEUE_SIZE>,
) -> ! {
    info!("Starting LV2 plugin host task");
    let mut plugin = PluginHost::load(midi_consumer);

    loop {
        let index = loop {
            if let Some(index) = free_consumer.dequeue() {
                break index;
            }
            embassy_futures::yield_now().await;
        };

        unsafe { plugin.process(block_mut_ptr(index)) };
        while ready_producer.enqueue(index).is_err() {
            embassy_futures::yield_now().await;
        }
    }
}

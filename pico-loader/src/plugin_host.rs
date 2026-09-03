use core::ffi::{CStr, c_char, c_void};

use defmt::info;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use heapless::spsc::{Consumer, Producer};
use lv2_bundle_format::{Bundle, FLASH_ADDRESS, MAX_SIZE};

use crate::audio_buffer::{
    AUDIO_QUEUE_SIZE, AudioBlockIndex, BLOCK_SIZE, MIDI_SCHEDULING_DELAY_BLOCKS, SAMPLE_RATE,
    block_mut_ptr,
};
use crate::lv2::{
    ATOM_SEQUENCE_URI, ATOM_SEQUENCE_URID, Lv2Descriptor, Lv2Feature, Lv2UridMap, MIDI_EVENT_URI,
    MIDI_EVENT_URID, URID_MAP_URI,
};
use crate::midi::{Lv2MidiSequence, MIDI_QUEUE_SIZE, MidiEvent};
use crate::plugin_metadata::{PluginMetadata, PortKind};

// const SYNTH_URI: &[u8] = b"https://joebutton.co.uk/lv2/tine-piano";
const SYNTH_URI: &[u8] = b"https://joebutton.co.uk/lv2/string-synth";
const DELAY_URI: &[u8] = b"https://joebutton.co.uk/lv2/delay-poc";

static mut MIDI_SEQUENCE: Lv2MidiSequence = Lv2MidiSequence::empty();
static mut SYNTH_AUDIO_BUFFER: [f32; BLOCK_SIZE] = [0.0; BLOCK_SIZE];
static mut DELAY_TIME: f32 = 0.100; // 100ms
static mut DELAY_FEEDBACK: f32 = 0.75; // high feedback for testing
static mut DELAY_DRY_WET: f32 = 0.5;

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
static mut FEATURES: [*const Lv2Feature; 2] =
    [core::ptr::addr_of!(URID_MAP_FEATURE), core::ptr::null()];

/// Represents a loaded, relocated LV2 plugin library binary.
pub struct PluginBinary {
    descriptor: &'static Lv2Descriptor,
}

impl PluginBinary {
    pub fn load(name: &str, elf_bytes: &[u8]) -> Self {
        let raw = Loader::new()
            .run()
            .load_dylib(ElfBinary::new(name, elf_bytes))
            .expect("failed to load lv2 plugin binary");
        let lib = Relocator::new()
            .run(raw)
            .relocate()
            .expect("failed to relocate lv2 plugin binary");

        let lv2_descriptor = unsafe {
            lib.get::<extern "C" fn(u32) -> *const Lv2Descriptor>("lv2_descriptor")
                .expect("symbol `lv2_descriptor` not found")
        };
        let descriptor: &'static Lv2Descriptor = unsafe { &*lv2_descriptor(0) };

        // Keep the relocated ELF resident in memory for the lifetime of the firmware
        core::mem::forget(lib);

        Self { descriptor }
    }

    pub fn instantiate(&self, sample_rate: f64, features: *const *const Lv2Feature) -> PluginInstance {
        let handle = (self.descriptor.instantiate)(
            self.descriptor,
            sample_rate,
            core::ptr::null(),
            features,
        );
        assert!(!handle.is_null(), "failed to instantiate lv2 plugin");

        PluginInstance {
            descriptor: self.descriptor,
            handle,
        }
    }
}

/// An active instance of a loaded LV2 plugin.
pub struct PluginInstance {
    descriptor: &'static Lv2Descriptor,
    handle: *mut c_void,
}

impl PluginInstance {
    pub fn connect_port(&mut self, port: u32, data_location: *mut c_void) {
        (self.descriptor.connect_port)(self.handle, port, data_location);
    }

    pub fn activate(&mut self) {
        (self.descriptor.activate)(self.handle);
    }

    pub fn run(&mut self, sample_count: u32) {
        (self.descriptor.run)(self.handle, sample_count);
    }

    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        (self.descriptor.deactivate)(self.handle);
    }
}

/// Manages loaded LV2 plugin instances and bridges queued MIDI events
/// through the audio processing pipeline.
pub struct PluginHost {
    synth: PluginInstance,
    delay: PluginInstance,
    delay_output_port: u32,
    midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>,
    pending_midi: Option<MidiEvent>,
    timeline_origin_micros: u64,
    block_start_frame: u64,
}

impl PluginHost {
    pub fn load(midi_consumer: Consumer<'static, MidiEvent, MIDI_QUEUE_SIZE>) -> Self {
        let bundle_bytes = unsafe {
            core::slice::from_raw_parts(FLASH_ADDRESS as *const u8, MAX_SIZE)
        };
        let bundle = Bundle::parse(bundle_bytes).expect("invalid plugin bundle");
        let synth_entry = bundle.find(SYNTH_URI).expect("synth plugin missing from bundle");
        let delay_entry = bundle.find(DELAY_URI).expect("delay plugin missing from bundle");
        let synth_binary = PluginBinary::load("synth-plugin.so", synth_entry.binary);
        let delay_binary = PluginBinary::load("delay-plugin.so", delay_entry.binary);
        let synth_metadata = PluginMetadata::parse(synth_entry.metadata).expect("invalid synth metadata");
        let delay_metadata = PluginMetadata::parse(delay_entry.metadata).expect("invalid delay metadata");

        let features_ptr = core::ptr::addr_of!(FEATURES) as *const *const Lv2Feature;

        let mut synth = synth_binary.instantiate(SAMPLE_RATE as f64, features_ptr);
        let mut delay = delay_binary.instantiate(SAMPLE_RATE as f64, features_ptr);

        synth.connect_port(
            synth_metadata.port(PortKind::AtomInput, 0).expect("synth MIDI port missing").index,
            core::ptr::addr_of_mut!(MIDI_SEQUENCE) as *mut c_void,
        );
        synth.connect_port(
            synth_metadata.port(PortKind::AudioOutput, 0).expect("synth output port missing").index,
            core::ptr::addr_of_mut!(SYNTH_AUDIO_BUFFER) as *mut c_void,
        );
        synth.activate();

        delay.connect_port(
            delay_metadata.port(PortKind::AudioInput, 0).expect("delay input port missing").index,
            core::ptr::addr_of_mut!(SYNTH_AUDIO_BUFFER) as *mut c_void,
        );
        let delay_output = delay_metadata.port(PortKind::AudioOutput, 0).expect("delay output port missing");
        let delay_controls = [
            delay_metadata.port(PortKind::ControlInput, 0).expect("delay control port missing"),
            delay_metadata.port(PortKind::ControlInput, 1).expect("delay control port missing"),
            delay_metadata.port(PortKind::ControlInput, 2).expect("delay control port missing"),
        ];
        unsafe {
            DELAY_TIME = delay_controls[0].default.expect("delay time default missing");
            DELAY_FEEDBACK = delay_controls[1].default.expect("delay feedback default missing");
            DELAY_DRY_WET = delay_controls[2].default.expect("delay dry/wet default missing");
        }
        delay.connect_port(
            delay_controls[0].index,
            core::ptr::addr_of_mut!(DELAY_TIME) as *mut c_void,
        );
        delay.connect_port(
            delay_controls[1].index,
            core::ptr::addr_of_mut!(DELAY_FEEDBACK) as *mut c_void,
        );
        delay.connect_port(
            delay_controls[2].index,
            core::ptr::addr_of_mut!(DELAY_DRY_WET) as *mut c_void,
        );
        delay.activate();

        Self {
            synth,
            delay,
            delay_output_port: delay_output.index,
            midi_consumer,
            pending_midi: None,
            timeline_origin_micros: embassy_time::Instant::now().as_micros(),
            block_start_frame: 0,
        }
    }

    unsafe fn process(&mut self, output: *mut f32) {
        let midi_sequence = unsafe { &mut *core::ptr::addr_of_mut!(MIDI_SEQUENCE) };
        let mut event_count = 0;
        while event_count < midi_sequence.events.len() {
            let Some(event) = self
                .pending_midi
                .take()
                .or_else(|| self.midi_consumer.dequeue())
            else {
                break;
            };

            // Schedule by reception time plus the maximum render-ahead depth:
            // static float-pool blocks plus packed I2S DMA buffers.
            let received_micros = event
                .timestamp_micros
                .saturating_sub(self.timeline_origin_micros);
            let received_frame = received_micros.saturating_mul(SAMPLE_RATE as u64) / 1_000_000;
            let target_frame = received_frame
                .saturating_add(MIDI_SCHEDULING_DELAY_BLOCKS as u64 * BLOCK_SIZE as u64);
            let block_end_frame = self.block_start_frame + BLOCK_SIZE as u64;
            if target_frame >= block_end_frame {
                self.pending_midi = Some(event);
                break;
            }

            midi_sequence.events[event_count].frame =
                target_frame.saturating_sub(self.block_start_frame) as i64;
            midi_sequence.events[event_count].message = [event.status, event.data1, event.data2];
            event_count += 1;
        }
        midi_sequence.set_event_count(event_count);

        // 1. Run synth plugin instance to render into synth intermediate buffer
        self.synth.run(BLOCK_SIZE as u32);

        // 2. Connect delay plugin output to the target audio buffer
        self.delay.connect_port(self.delay_output_port, output as *mut c_void);

        // 3. Run delay plugin instance to render into final output buffer
        self.delay.run(BLOCK_SIZE as u32);

        self.block_start_frame += BLOCK_SIZE as u64;
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

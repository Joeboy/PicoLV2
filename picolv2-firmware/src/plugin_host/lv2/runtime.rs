use core::ffi::{CStr, c_char, c_void};

use defmt::info;
use elf_loader::{
    Loader, Relocator,
    image::{SyntheticModule, SyntheticSymbol},
    input::ElfBinary,
};

use super::host_abi::HOST_SYMBOLS;
use super::{
    ATOM_BLANK_URI, ATOM_BLANK_URID, ATOM_DOUBLE_URI, ATOM_DOUBLE_URID, ATOM_FLOAT_URI,
    ATOM_FLOAT_URID, ATOM_INT_URI, ATOM_INT_URID, ATOM_LONG_URI, ATOM_LONG_URID, ATOM_OBJECT_URI,
    ATOM_OBJECT_URID, ATOM_SEQUENCE_URI, ATOM_SEQUENCE_URID, Lv2Descriptor, Lv2Feature, Lv2UridMap,
    MIDI_EVENT_URI, MIDI_EVENT_URID, TIME_BAR_BEAT_URI, TIME_BAR_BEAT_URID, TIME_BAR_URI,
    TIME_BAR_URID, TIME_BEAT_UNIT_URI, TIME_BEAT_UNIT_URID, TIME_BEATS_PER_BAR_URI,
    TIME_BEATS_PER_BAR_URID, TIME_BEATS_PER_MINUTE_URI, TIME_BEATS_PER_MINUTE_URID, TIME_FRAME_URI,
    TIME_FRAME_URID, TIME_POSITION_URI, TIME_POSITION_URID, TIME_SPEED_URI, TIME_SPEED_URID,
    URID_MAP_URI,
};
use crate::log_heap;

extern "C" fn map_uri(_handle: *mut c_void, uri: *const c_char) -> u32 {
    if uri.is_null() {
        return 0;
    }

    let uri = unsafe { CStr::from_ptr(uri) }.to_bytes_with_nul();
    match uri {
        ATOM_SEQUENCE_URI => ATOM_SEQUENCE_URID,
        MIDI_EVENT_URI => MIDI_EVENT_URID,
        ATOM_BLANK_URI => ATOM_BLANK_URID,
        ATOM_OBJECT_URI => ATOM_OBJECT_URID,
        ATOM_DOUBLE_URI => ATOM_DOUBLE_URID,
        ATOM_FLOAT_URI => ATOM_FLOAT_URID,
        ATOM_INT_URI => ATOM_INT_URID,
        ATOM_LONG_URI => ATOM_LONG_URID,
        TIME_POSITION_URI => TIME_POSITION_URID,
        TIME_BAR_URI => TIME_BAR_URID,
        TIME_BAR_BEAT_URI => TIME_BAR_BEAT_URID,
        TIME_BEATS_PER_BAR_URI => TIME_BEATS_PER_BAR_URID,
        TIME_BEATS_PER_MINUTE_URI => TIME_BEATS_PER_MINUTE_URID,
        TIME_BEAT_UNIT_URI => TIME_BEAT_UNIT_URID,
        TIME_FRAME_URI => TIME_FRAME_URID,
        TIME_SPEED_URI => TIME_SPEED_URID,
        _ => 0,
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

pub(in crate::plugin_host) fn features_ptr() -> *const *const Lv2Feature {
    core::ptr::addr_of!(FEATURES) as *const *const Lv2Feature
}

/// Represents a loaded, relocated LV2 plugin library binary.
pub(in crate::plugin_host) struct PluginBinary {
    descriptor: &'static Lv2Descriptor,
}

impl PluginBinary {
    pub(in crate::plugin_host) fn load(name: &str, elf_bytes: &[u8], plugin_uri: &[u8]) -> Self {
        info!(
            "plugin load begin name={} elf_bytes={}",
            name,
            elf_bytes.len()
        );
        log_heap("before elf load");
        let raw = Loader::new()
            .run()
            .load_dylib(ElfBinary::new(name, elf_bytes))
            .expect("failed to load lv2 plugin binary");
        log_heap("after elf load");
        let host = SyntheticModule::new(
            "picolv2-host",
            HOST_SYMBOLS.map(|(name, address)| SyntheticSymbol::function(name, address)),
        );
        let lib = Relocator::new()
            .run(raw)
            .modules([host])
            .relocate()
            .expect("failed to relocate lv2 plugin binary");
        lib.initialize()
            .expect("failed to initialize lv2 plugin binary");
        log_heap("after relocation");

        let lv2_descriptor = unsafe {
            lib.get::<extern "C" fn(u32) -> *const Lv2Descriptor>("lv2_descriptor")
                .expect("symbol `lv2_descriptor` not found")
        };
        let mut descriptor_index = 0;
        let descriptor: &'static Lv2Descriptor = loop {
            let candidate = lv2_descriptor(descriptor_index);
            assert!(
                !candidate.is_null(),
                "plugin binary does not contain the requested LV2 descriptor"
            );
            let candidate = unsafe { &*candidate };
            assert!(!candidate.uri.is_null(), "LV2 descriptor has a null URI");
            if unsafe { CStr::from_ptr(candidate.uri) }.to_bytes() == plugin_uri {
                break candidate;
            }
            descriptor_index += 1;
        };
        info!("selected LV2 descriptor index={}", descriptor_index);

        // Keep the relocated ELF resident in memory for the lifetime of the firmware.
        core::mem::forget(lib);
        Self { descriptor }
    }

    pub(in crate::plugin_host) fn instantiate(
        &self,
        sample_rate: f64,
        features: *const *const Lv2Feature,
    ) -> PluginInstance {
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
pub(in crate::plugin_host) struct PluginInstance {
    descriptor: &'static Lv2Descriptor,
    handle: *mut c_void,
}

impl PluginInstance {
    pub(in crate::plugin_host) fn connect_port(&mut self, port: u32, data_location: *mut c_void) {
        (self.descriptor.connect_port)(self.handle, port, data_location);
    }

    pub(in crate::plugin_host) fn activate(&mut self) {
        if let Some(activate) = self.descriptor.activate {
            activate(self.handle);
        }
    }

    pub(in crate::plugin_host) fn run(&mut self, sample_count: u32) {
        (self.descriptor.run)(self.handle, sample_count);
    }

    #[allow(dead_code)]
    pub(in crate::plugin_host) fn deactivate(&mut self) {
        if let Some(deactivate) = self.descriptor.deactivate {
            deactivate(self.handle);
        }
    }
}

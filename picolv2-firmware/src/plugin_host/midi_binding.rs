use alloc::vec::Vec;
use picolv2_image_format::{
    Graph, MIDI_BINDING_INTEGER, MIDI_BINDING_LOGARITHMIC, MIDI_BINDING_TOGGLED,
    MIDI_BINDING_TRIGGER,
};

use super::plugin_graph::PluginNode;
use crate::midi::MidiEvent;

pub(super) struct MidiControlBinding {
    pub(super) channel: u8,
    pub(super) controller: u8,
    pub(super) flags: u8,
    pub(super) minimum: f32,
    pub(super) maximum: f32,
    pub(super) target: *mut f32,
}

pub(super) fn load_bindings(
    graph: &Graph<'_>,
    nodes: &mut [PluginNode],
) -> Vec<MidiControlBinding> {
    let mut bindings = Vec::new();
    for binding_index in 0..graph.midi_binding_count {
        let binding = graph
            .midi_binding(binding_index)
            .expect("invalid MIDI binding");
        assert!(binding.channel < 16, "MIDI binding channel out of range");
        assert!(
            binding.controller < 128,
            "MIDI binding controller out of range"
        );
        let node = nodes
            .get_mut(binding.node as usize)
            .expect("MIDI binding node out of range");
        let target = node
            .control_inputs
            .iter_mut()
            .find(|(port, _)| *port == binding.port as u32)
            .map(|(_, control)| control.as_mut() as *mut f32)
            .expect("MIDI binding target is not a control input");
        bindings.push(MidiControlBinding {
            channel: binding.channel,
            controller: binding.controller,
            flags: binding.flags,
            minimum: binding.minimum,
            maximum: binding.maximum,
            target,
        });
    }
    bindings
}

impl MidiControlBinding {
    pub(super) fn apply(&self, event: MidiEvent) {
        if event.status & 0xf0 != 0xb0
            || event.status & 0x0f != self.channel
            || event.data1 != self.controller
        {
            return;
        }
        let mut value = if self.flags & MIDI_BINDING_TRIGGER != 0 {
            self.maximum
        } else if self.flags & MIDI_BINDING_TOGGLED != 0 {
            if event.data2 >= 64 {
                self.maximum
            } else {
                self.minimum
            }
        } else if event.data2 == 0 {
            self.minimum
        } else if event.data2 == 127 {
            self.maximum
        } else {
            let normalized = f32::from(event.data2) / 127.0;
            if self.flags & MIDI_BINDING_LOGARITHMIC != 0 {
                self.minimum * libm::powf(self.maximum / self.minimum, normalized)
            } else {
                self.minimum + (self.maximum - self.minimum) * normalized
            }
        };
        if self.flags & MIDI_BINDING_INTEGER != 0 {
            value = libm::roundf(value);
        }
        unsafe { *self.target = value };
    }

    pub(super) fn reset_trigger(&self) {
        if self.flags & MIDI_BINDING_TRIGGER != 0 {
            unsafe { *self.target = self.minimum };
        }
    }
}

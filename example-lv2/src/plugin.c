#include <stdint.h>

// A minimal, self-contained subset of the real LV2 plugin ABI
// (https://lv2plug.in/ns/lv2core), reimplemented here rather than pulling in
// the actual lv2.h header, since this is a bare-metal freestanding build.
// The shape (URI + a table of lifecycle function pointers, discovered via a
// `lv2_descriptor(index)` entry point) matches the real spec.

typedef void *LV2_Handle;

typedef struct LV2_Descriptor {
    const char *uri;
    LV2_Handle (*instantiate)(const struct LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const void *const *features);
    void (*connect_port)(LV2_Handle instance, uint32_t port, void *data_location);
    void (*activate)(LV2_Handle instance);
    void (*run)(LV2_Handle instance, uint32_t sample_count);
    void (*deactivate)(LV2_Handle instance);
    void (*cleanup)(LV2_Handle instance);
    const void *(*extension_data)(const char *uri);
} LV2_Descriptor;

// Port indices for this plugin: one audio output and a host-provided block of
// MIDI events.
#define PORT_OUTPUT 0
#define PORT_MIDI_IN 1

#define AMPLITUDE 0.8f

// Equal-tempered ratios for semitones C through B within one octave. Using a
// small table avoids depending on libm/powf in the freestanding Pico build.
static const float SEMITONE_RATIOS[12] = {
    1.0f,
    1.059463094f,
    1.122462048f,
    1.189207115f,
    1.259921050f,
    1.334839854f,
    1.414213562f,
    1.498307077f,
    1.587401052f,
    1.681792831f,
    1.781797436f,
    1.887748625f,
};

typedef struct {
    uint8_t status;
    uint8_t data1;
    uint8_t data2;
    uint8_t reserved;
} MidiEvent;

typedef struct {
    const MidiEvent *events;
    uint32_t event_count;
} MidiEventBlock;

// Freestanding build, no malloc: a single static instance is enough for our
// purposes (we only ever instantiate one copy of this plugin at a time).
typedef struct {
    // Converted from the LV2-mandated `double` once here, rather than kept
    // as a double, so the real-time `run()` path stays on hardware
    // single-precision float instead of libgcc's soft-double routines.
    float sample_rate;
    float *output;
    const MidiEventBlock *midi_in;
    float note_frequency;
    float velocity_gain;
    uint8_t active_note;
    uint8_t note_on;
    // Position within the current cycle, in samples.
    uint32_t phase;
} SynthState;

static SynthState g_state;

static LV2_Handle instantiate(const LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const void *const *features) {
    (void)descriptor;
    (void)bundle_path;
    (void)features;
    g_state.sample_rate = (float)sample_rate;
    g_state.output = 0;
    g_state.midi_in = 0;
    g_state.note_frequency = 440.0f;
    g_state.velocity_gain = 0.0f;
    g_state.active_note = 0;
    g_state.note_on = 0;
    g_state.phase = 0;
    return &g_state;
}

static void connect_port(LV2_Handle instance, uint32_t port, void *data_location) {
    SynthState *synth = (SynthState *)instance;
    switch (port) {
        case PORT_OUTPUT:
            synth->output = (float *)data_location;
            break;
        case PORT_MIDI_IN:
            synth->midi_in = (const MidiEventBlock *)data_location;
            break;
    }
}

static void activate(LV2_Handle instance) {
    SynthState *synth = (SynthState *)instance;
    synth->phase = 0;
    synth->note_on = 0;
    synth->velocity_gain = 0.0f;
}

static float midi_note_frequency(uint8_t note) {
    // MIDI note 60 is middle C (C4), approximately 261.626 Hz.
    int32_t octave = ((int32_t)note / 12) - 5;
    float frequency = 261.625565f * SEMITONE_RATIOS[note % 12];
    while (octave > 0) {
        frequency *= 2.0f;
        octave--;
    }
    while (octave < 0) {
        frequency *= 0.5f;
        octave++;
    }
    return frequency;
}

static void handle_midi_event(SynthState *synth, const MidiEvent *event) {
    uint8_t message = event->status & 0xf0;
    if (message == 0x90 && event->data2 != 0) {
        synth->active_note = event->data1;
        synth->note_frequency = midi_note_frequency(event->data1);
        synth->velocity_gain = (float)event->data2 / 127.0f;
        synth->note_on = 1;
        synth->phase = 0;
    } else if ((message == 0x80 || message == 0x90) &&
               event->data1 == synth->active_note) {
        synth->note_on = 0;
        synth->velocity_gain = 0.0f;
    } else if (message == 0xb0 &&
               (event->data1 == 120 || event->data1 == 123)) {
        synth->note_on = 0;
        synth->velocity_gain = 0.0f;
    }
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    SynthState *synth = (SynthState *)instance;
    if (synth->midi_in && synth->midi_in->events) {
        for (uint32_t i = 0; i < synth->midi_in->event_count; i++) {
            const MidiEvent *event = &synth->midi_in->events[i];
            handle_midi_event(synth, event);
        }
    }

    float freq = synth->note_frequency;
    uint32_t period_samples = (uint32_t)(synth->sample_rate / freq);
    if (period_samples < 2) {
        period_samples = 2;
    }
    uint32_t half_period = period_samples / 2;

    for (uint32_t i = 0; i < sample_count; i++) {
        if (!synth->note_on) {
            synth->output[i] = 0.0f;
            continue;
        }

        float amplitude = AMPLITUDE * synth->velocity_gain;
        synth->output[i] = (synth->phase < half_period) ? amplitude : -amplitude;
        synth->phase++;
        if (synth->phase >= period_samples) {
            synth->phase = 0;
        }
    }
}

static void deactivate(LV2_Handle instance) {
    (void)instance;
}

static void cleanup(LV2_Handle instance) {
    (void)instance;
}

static const void *extension_data(const char *uri) {
    (void)uri;
    return 0;
}

static const LV2_Descriptor descriptor = {
    "https://joebutton.co.uk/lv2/square-synth-poc",
    instantiate,
    connect_port,
    activate,
    run,
    deactivate,
    cleanup,
    extension_data,
};


const LV2_Descriptor *lv2_descriptor(uint32_t index) {
    if (index == 0) {
        return &descriptor;
    }
    return 0;
}

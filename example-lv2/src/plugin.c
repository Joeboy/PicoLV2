#include <stdint.h>

// A minimal, self-contained subset of the real LV2 plugin ABI
// (https://lv2plug.in/ns/lv2core), reimplemented here rather than pulling in
// the actual lv2.h header, since this is a bare-metal freestanding build.
// The shape (URI + a table of lifecycle function pointers, discovered via a
// `lv2_descriptor(index)` entry point) matches the real spec.

typedef void *LV2_Handle;

typedef struct {
    const char *uri;
    void *data;
} LV2_Feature;

typedef struct {
    void *handle;
    uint32_t (*map)(void *handle, const char *uri);
} LV2_URID_Map;

typedef struct LV2_Descriptor {
    const char *uri;
    LV2_Handle (*instantiate)(const struct LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const LV2_Feature *const *features);
    void (*connect_port)(LV2_Handle instance, uint32_t port, void *data_location);
    void (*activate)(LV2_Handle instance);
    void (*run)(LV2_Handle instance, uint32_t sample_count);
    void (*deactivate)(LV2_Handle instance);
    void (*cleanup)(LV2_Handle instance);
    const void *(*extension_data)(const char *uri);
} LV2_Descriptor;

// Port indices for this plugin: one audio output and an LV2 Atom Sequence MIDI
// input.
#define PORT_MIDI_IN 0
#define PORT_OUTPUT 1

#define LV2_URID__map "http://lv2plug.in/ns/ext/urid#map"
#define LV2_ATOM__Sequence "http://lv2plug.in/ns/ext/atom#Sequence"
#define LV2_MIDI__MidiEvent "http://lv2plug.in/ns/ext/midi#MidiEvent"

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
    uint32_t size;
    uint32_t type;
} LV2_Atom;

typedef struct {
    uint32_t unit;
    uint32_t pad;
} LV2_Atom_Sequence_Body;

typedef struct {
    LV2_Atom atom;
    LV2_Atom_Sequence_Body body;
} LV2_Atom_Sequence;

typedef struct {
    int64_t frames;
    LV2_Atom body;
} LV2_Atom_Event;

// Freestanding build, no malloc: a single static instance is enough for our
// purposes (we only ever instantiate one copy of this plugin at a time).
typedef struct {
    // Converted from the LV2-mandated `double` once here, rather than kept
    // as a double, so the real-time `run()` path stays on hardware
    // single-precision float instead of libgcc's soft-double routines.
    float sample_rate;
    float *output;
    const LV2_Atom_Sequence *midi_in;
    float note_frequency;
    float velocity_gain;
    uint8_t active_note;
    uint8_t note_on;
    uint32_t atom_sequence_urid;
    uint32_t midi_event_urid;
    // Position within the current cycle, in samples.
    uint32_t phase;
} SynthState;

static SynthState g_state;

static int strings_equal(const char *a, const char *b) {
    while (*a && *a == *b) {
        a++;
        b++;
    }
    return *a == *b;
}

static LV2_Handle instantiate(const LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const LV2_Feature *const *features) {
    (void)descriptor;
    (void)bundle_path;

    const LV2_URID_Map *map = 0;
    if (features) {
        for (uint32_t i = 0; features[i]; i++) {
            if (strings_equal(features[i]->uri, LV2_URID__map)) {
                map = (const LV2_URID_Map *)features[i]->data;
                break;
            }
        }
    }
    if (!map || !map->map) {
        return 0;
    }

    uint32_t atom_sequence_urid = map->map(map->handle, LV2_ATOM__Sequence);
    uint32_t midi_event_urid = map->map(map->handle, LV2_MIDI__MidiEvent);
    if (!atom_sequence_urid || !midi_event_urid) {
        return 0;
    }

    g_state.sample_rate = (float)sample_rate;
    g_state.output = 0;
    g_state.midi_in = 0;
    g_state.note_frequency = 440.0f;
    g_state.velocity_gain = 0.0f;
    g_state.active_note = 0;
    g_state.note_on = 0;
    g_state.atom_sequence_urid = atom_sequence_urid;
    g_state.midi_event_urid = midi_event_urid;
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
            synth->midi_in = (const LV2_Atom_Sequence *)data_location;
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

static void handle_midi_event(SynthState *synth, const uint8_t *message_data) {
    uint8_t message = message_data[0] & 0xf0;
    if (message == 0x90 && message_data[2] != 0) {
        synth->active_note = message_data[1];
        synth->note_frequency = midi_note_frequency(message_data[1]);
        synth->velocity_gain = (float)message_data[2] / 127.0f;
        synth->note_on = 1;
        synth->phase = 0;
    } else if ((message == 0x80 || message == 0x90) &&
               message_data[1] == synth->active_note) {
        synth->note_on = 0;
        synth->velocity_gain = 0.0f;
    } else if (message == 0xb0 &&
               (message_data[1] == 120 || message_data[1] == 123)) {
        synth->note_on = 0;
        synth->velocity_gain = 0.0f;
    }
}

static uint32_t pad_size(uint32_t size) {
    return (size + 7u) & ~7u;
}

static void render(SynthState *synth, uint32_t start, uint32_t end) {
    float freq = synth->note_frequency;
    uint32_t period_samples = (uint32_t)(synth->sample_rate / freq);
    if (period_samples < 2) {
        period_samples = 2;
    }
    uint32_t half_period = period_samples / 2;
    float amplitude = AMPLITUDE * synth->velocity_gain;

    for (uint32_t i = start; i < end; i++) {
        if (!synth->note_on) {
            synth->output[i] = 0.0f;
            continue;
        }

        synth->output[i] = (synth->phase < half_period) ? amplitude : -amplitude;
        synth->phase++;
        if (synth->phase >= period_samples) {
            synth->phase = 0;
        }
    }
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    SynthState *synth = (SynthState *)instance;
    uint32_t offset = 0;
    if (synth->midi_in &&
        synth->midi_in->atom.type == synth->atom_sequence_urid &&
        synth->midi_in->atom.size >= sizeof(LV2_Atom_Sequence_Body)) {
        const uint8_t *event_ptr = (const uint8_t *)(&synth->midi_in->body + 1);
        const uint8_t *end = (const uint8_t *)&synth->midi_in->body +
                             synth->midi_in->atom.size;
        while (event_ptr + sizeof(LV2_Atom_Event) <= end) {
            const LV2_Atom_Event *event = (const LV2_Atom_Event *)event_ptr;
            uint32_t event_size = sizeof(LV2_Atom_Event) + pad_size(event->body.size);
            if (event_ptr + event_size > end) {
                break;
            }
            if (event->body.type == synth->midi_event_urid &&
                event->body.size >= 3) {
                uint32_t event_frame = event->frames < 0 ? 0 : (uint32_t)event->frames;
                if (event_frame > sample_count) {
                    event_frame = sample_count;
                }
                if (event_frame < offset) {
                    event_frame = offset;
                }
                render(synth, offset, event_frame);
                handle_midi_event(synth, (const uint8_t *)(event + 1));
                offset = event_frame;
            }
            event_ptr += event_size;
        }
    }
    render(synth, offset, sample_count);
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

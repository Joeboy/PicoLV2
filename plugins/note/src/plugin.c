#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <math.h>

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

#define PORT_MIDI_IN 0
#define PORT_FREQUENCY_OUT 1
#define PORT_NOTE_OUT 2
#define PORT_VELOCITY_OUT 3
#define PORT_GATE_OUT 4
#define PORT_TRIGGER_OUT 5

#define LV2_URID__map "http://lv2plug.in/ns/ext/urid#map"
#define LV2_ATOM__Sequence "http://lv2plug.in/ns/ext/atom#Sequence"
#define LV2_MIDI__MidiEvent "http://lv2plug.in/ns/ext/midi#MidiEvent"

#define NOTE_STATE_IDLE 0
#define NOTE_STATE_ACTIVE 1

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

typedef struct {
    uint8_t note;
    uint8_t velocity;
    float frequency;
    float gate;
    float trigger;
    uint8_t state;
    uint32_t atom_sequence_urid;
    uint32_t midi_event_urid;
    const LV2_Atom_Sequence *midi_in;
    float *frequency_out;
    float *note_out;
    float *velocity_out;
    float *gate_out;
    float *trigger_out;
} NoteState;

static int strings_equal(const char *a, const char *b) {
    while (*a && *a == *b) {
        a++;
        b++;
    }
    return *a == *b;
}

static float midi_note_frequency(uint8_t note) {
    static const float ratios[12] = {
        16.3515978f, 17.3239144f, 18.3540479f, 19.4454365f, 20.6017220f,
        21.8267645f, 23.1246514f, 24.4997147f, 25.9565432f, 27.5000000f,
        29.1352351f, 30.8677063f,
    };

    int octave = (int)note / 12 - 5;
    float frequency = 27.5f * ratios[note % 12];

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

static LV2_Handle instantiate(const LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const LV2_Feature *const *features) {
    (void)descriptor;
    (void)sample_rate;
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

    NoteState *state = (NoteState *)calloc(1, sizeof(NoteState));
    if (!state) {
        return 0;
    }

    state->note = 0;
    state->velocity = 0;
    state->frequency = 0.0f;
    state->gate = 0.0f;
    state->trigger = 0.0f;
    state->state = NOTE_STATE_IDLE;
    state->atom_sequence_urid = atom_sequence_urid;
    state->midi_event_urid = midi_event_urid;
    state->midi_in = 0;
    state->frequency_out = 0;
    state->note_out = 0;
    state->velocity_out = 0;
    state->gate_out = 0;
    state->trigger_out = 0;
    return state;
}

static void connect_port(LV2_Handle instance, uint32_t port, void *data_location) {
    NoteState *state = (NoteState *)instance;
    switch (port) {
        case PORT_MIDI_IN:
            state->midi_in = (const LV2_Atom_Sequence *)data_location;
            break;
        case PORT_FREQUENCY_OUT:
            state->frequency_out = (float *)data_location;
            break;
        case PORT_NOTE_OUT:
            state->note_out = (float *)data_location;
            break;
        case PORT_VELOCITY_OUT:
            state->velocity_out = (float *)data_location;
            break;
        case PORT_GATE_OUT:
            state->gate_out = (float *)data_location;
            break;
        case PORT_TRIGGER_OUT:
            state->trigger_out = (float *)data_location;
            break;
    }
}

static void activate(LV2_Handle instance) {
    NoteState *state = (NoteState *)instance;
    state->note = 0;
    state->velocity = 0;
    state->frequency = 0.0f;
    state->gate = 0.0f;
    state->trigger = 0.0f;
    state->state = NOTE_STATE_IDLE;
}

static uint32_t pad_size(uint32_t size) {
    return (size + 7u) & ~7u;
}

static void emit_note_state(NoteState *state, const char *event_name, uint8_t note, uint8_t velocity) {
    state->note = note;
    state->velocity = velocity;
    state->frequency = midi_note_frequency(note);

    if (event_name[0] == 'o') {
        state->state = NOTE_STATE_ACTIVE;
        state->gate = 1.0f;
        state->trigger = 1.0f;
        printf("note: ON  note=%u velocity=%u freq=%.2f Hz\n", note, velocity, state->frequency);
    } else {
        state->state = NOTE_STATE_IDLE;
        state->gate = 0.0f;
        state->trigger = 0.0f;
        printf("note: OFF note=%u velocity=%u\n", note, velocity);
    }
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    (void)sample_count;

    NoteState *state = (NoteState *)instance;
    if (!state->midi_in) {
        return;
    }

    if (state->midi_in->atom.type == state->atom_sequence_urid &&
        state->midi_in->atom.size >= sizeof(LV2_Atom_Sequence_Body)) {
        const uint8_t *event_ptr = (const uint8_t *)(&state->midi_in->body + 1);
        const uint8_t *end = (const uint8_t *)&state->midi_in->body +
                             state->midi_in->atom.size;

        while (event_ptr + sizeof(LV2_Atom_Event) <= end) {
            const LV2_Atom_Event *event = (const LV2_Atom_Event *)event_ptr;
            uint32_t event_size = sizeof(LV2_Atom_Event) + pad_size(event->body.size);
            if (event_ptr + event_size > end) {
                break;
            }

            if (event->body.type == state->midi_event_urid && event->body.size >= 3) {
                const uint8_t *message = (const uint8_t *)(event + 1);
                uint8_t status = message[0] & 0xf0;
                uint8_t note = message[1];
                uint8_t velocity = message[2];

                if (status == 0x90 && velocity != 0) {
                    emit_note_state(state, "on", note, velocity);
                } else if ((status == 0x80 || (status == 0x90 && velocity == 0))) {
                    emit_note_state(state, "off", note, velocity);
                } else if (status == 0xb0 && (note == 120 || note == 123)) {
                    state->state = NOTE_STATE_IDLE;
                    state->gate = 0.0f;
                    state->trigger = 0.0f;
                    state->note = note;
                    printf("note: ALL_OFF controller=%u\n", note);
                }
            }
            event_ptr += event_size;
        }
    }

    if (state->frequency_out) {
        *state->frequency_out = state->frequency;
    }
    if (state->note_out) {
        *state->note_out = (float)state->note;
    }
    if (state->velocity_out) {
        *state->velocity_out = (float)state->velocity / 127.0f;
    }
    if (state->gate_out) {
        *state->gate_out = state->gate;
    }
    if (state->trigger_out) {
        *state->trigger_out = state->trigger;
        state->trigger = 0.0f;
    }
}

static void deactivate(LV2_Handle instance) {
    (void)instance;
}

static void cleanup(LV2_Handle instance) {
    free(instance);
}

static const void *extension_data(const char *uri) {
    (void)uri;
    return 0;
}

static const LV2_Descriptor descriptor = {
    "http://drobilla.net/ns/ingen-internals#Note",
    instantiate,
    connect_port,
    activate,
    run,
    deactivate,
    cleanup,
    extension_data,
};

const LV2_Descriptor *lv2_descriptor(uint32_t index) {
    return index == 0 ? &descriptor : 0;
}

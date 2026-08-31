#include <stdint.h>

#define PLUGIN_URI "https://joebutton.co.uk/lv2/oxynth-poc"
#define LV2_URID__map "http://lv2plug.in/ns/ext/urid#map"
#define LV2_ATOM__Sequence "http://lv2plug.in/ns/ext/atom#Sequence"
#define LV2_MIDI__MidiEvent "http://lv2plug.in/ns/ext/midi#MidiEvent"

#define PORT_MIDI_IN 0
#define PORT_OUTPUT 1
#define N_VOICES 16
#define PI 3.14159265358979323846f
#define TWO_PI (2.0f * PI)
#define MAX_AMPLITUDE (12000.0f / 32767.0f)

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
    LV2_Handle (*instantiate)(const struct LV2_Descriptor *, double, const char *,
                              const LV2_Feature *const *);
    void (*connect_port)(LV2_Handle, uint32_t, void *);
    void (*activate)(LV2_Handle);
    void (*run)(LV2_Handle, uint32_t);
    void (*deactivate)(LV2_Handle);
    void (*cleanup)(LV2_Handle);
    const void *(*extension_data)(const char *);
} LV2_Descriptor;

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

typedef enum {
    WAVE_SINE,
    WAVE_SQUARE,
    WAVE_SAWTOOTH,
    WAVE_TRIANGLE,
} Waveform;

typedef enum {
    ENV_IDLE,
    ENV_ATTACK,
    ENV_DECAY,
    ENV_SUSTAIN,
    ENV_RELEASE,
} EnvStage;

typedef struct {
    uint8_t note;
    float frequency;
    float target_amplitude;
    float envelope;
    uint8_t gate;
    float phase;
    uint32_t age;
    EnvStage stage;
    float attack_increment;
    float decay_increment;
    float sustain_level;
    float release_increment;
    float filter_bandpass;
    float filter_lowpass;
} Voice;

typedef struct {
    float sample_rate;
    const LV2_Atom_Sequence *midi_in;
    float *output;
    uint32_t atom_sequence_urid;
    uint32_t midi_event_urid;
    Voice voices[N_VOICES];
    uint32_t age_counter;
    Waveform waveform;
    float attack_seconds;
    float decay_seconds;
    float sustain_level;
    float release_seconds;
    float filter_cutoff;
    float filter_resonance;
} Synth;

static Synth instance;

static const float SEMITONE_RATIOS[12] = {
    1.0f, 1.059463094f, 1.122462048f, 1.189207115f,
    1.259921050f, 1.334839854f, 1.414213562f, 1.498307077f,
    1.587401052f, 1.681792831f, 1.781797436f, 1.887748625f,
};

static int strings_equal(const char *a, const char *b) {
    while (*a && *a == *b) {
        ++a;
        ++b;
    }
    return *a == *b;
}

static float minimum(float a, float b) {
    return a < b ? a : b;
}

static float maximum(float a, float b) {
    return a > b ? a : b;
}

static float midi_note_frequency(uint8_t note) {
    int32_t octave = ((int32_t)note / 12) - 5;
    float frequency = 261.625565f * SEMITONE_RATIOS[note % 12];
    while (octave > 0) {
        frequency *= 2.0f;
        --octave;
    }
    while (octave < 0) {
        frequency *= 0.5f;
        ++octave;
    }
    return frequency;
}

// A small sine approximation suitable for the original synth's selectable
// oscillator. Input phase is in [0, 1).
static float sine_wave(float phase) {
    float x = phase * TWO_PI - PI;
    float x2 = x * x;
    return x * (1.0f - x2 * (1.0f / 6.0f) + x2 * x2 * (1.0f / 120.0f) -
                x2 * x2 * x2 * (1.0f / 5040.0f));
}

static int voice_active(const Voice *voice) {
    return voice->stage != ENV_IDLE || voice->envelope > 0.000001f;
}

__attribute__((noinline)) static void reset_voice(Voice *voice) {
    voice->note = 0;
    voice->frequency = 0.0f;
    voice->target_amplitude = 0.0f;
    voice->envelope = 0.0f;
    voice->gate = 0;
    voice->phase = 0.0f;
    voice->age = 0;
    voice->stage = ENV_IDLE;
    voice->attack_increment = 0.0f;
    voice->decay_increment = 0.0f;
    voice->sustain_level = 1.0f;
    voice->release_increment = 0.0f;
    voice->filter_bandpass = 0.0f;
    voice->filter_lowpass = 0.0f;
}

static void start_voice(Synth *synth, Voice *voice, uint8_t note, uint8_t velocity) {
    float amplitude = (float)velocity / 127.0f;
    float attack_samples = maximum(synth->attack_seconds * synth->sample_rate, 1.0f);
    float decay_samples = maximum(synth->decay_seconds * synth->sample_rate, 1.0f);

    voice->note = note;
    voice->frequency = midi_note_frequency(note);
    voice->target_amplitude = amplitude;
    voice->gate = 1;
    voice->age = ++synth->age_counter;
    voice->sustain_level = synth->sustain_level;
    voice->attack_increment = amplitude / attack_samples;
    voice->decay_increment =
        (amplitude - synth->sustain_level * amplitude) / decay_samples;
    voice->release_increment = 0.0f;
    voice->stage = ENV_ATTACK;
}

static void release_voice(const Synth *synth, Voice *voice) {
    float release_samples = maximum(synth->release_seconds * synth->sample_rate, 1.0f);
    voice->gate = 0;
    voice->release_increment = voice->envelope / release_samples;
    voice->stage = ENV_RELEASE;
}

static Voice *allocate_voice(Synth *synth) {
    Voice *oldest = &synth->voices[0];
    for (uint32_t i = 0; i < N_VOICES; ++i) {
        if (!voice_active(&synth->voices[i])) {
            return &synth->voices[i];
        }
        if (synth->voices[i].age < oldest->age) {
            oldest = &synth->voices[i];
        }
    }
    return oldest;
}

static void handle_midi(Synth *synth, const uint8_t *message) {
    uint8_t kind = message[0] & 0xf0;
    uint8_t data1 = message[1];
    uint8_t data2 = message[2];

    if (kind == 0x90 && data2 != 0) {
        start_voice(synth, allocate_voice(synth), data1, data2);
    } else if (kind == 0x80 || (kind == 0x90 && data2 == 0)) {
        for (uint32_t i = 0; i < N_VOICES; ++i) {
            if (synth->voices[i].note == data1 && synth->voices[i].gate) {
                release_voice(synth, &synth->voices[i]);
            }
        }
    } else if (kind == 0xb0) {
        switch (data1) {
            case 21:
                synth->waveform = (Waveform)(data2 / 32u);
                if (synth->waveform > WAVE_TRIANGLE) {
                    synth->waveform = WAVE_TRIANGLE;
                }
                break;
            case 22:
                synth->attack_seconds = 0.001f + ((float)data2 / 127.0f) * 1.999f;
                break;
            case 23:
                synth->decay_seconds = 0.001f + ((float)data2 / 127.0f) * 1.999f;
                break;
            case 24:
                synth->sustain_level = (float)data2 / 127.0f;
                break;
            case 25:
                synth->release_seconds = 0.001f + ((float)data2 / 127.0f) * 2.999f;
                break;
            case 26:
                synth->filter_cutoff = (float)data2 / 127.0f;
                break;
            case 27:
                synth->filter_resonance = ((float)data2 / 127.0f) * 4.0f;
                break;
            case 120:
            case 123:
                for (uint32_t i = 0; i < N_VOICES; ++i) {
                    if (synth->voices[i].gate) {
                        release_voice(synth, &synth->voices[i]);
                    }
                }
                break;
            default:
                break;
        }
    }
}

static float oscillator_sample(Waveform waveform, float phase) {
    switch (waveform) {
        case WAVE_SINE:
            return sine_wave(phase);
        case WAVE_SQUARE:
            return phase < 0.5f ? 1.0f : -1.0f;
        case WAVE_SAWTOOTH:
            return 2.0f * phase - 1.0f;
        case WAVE_TRIANGLE:
            return phase < 0.5f ? 4.0f * phase - 1.0f : 3.0f - 4.0f * phase;
    }
    return 0.0f;
}

static void advance_envelope(Voice *voice) {
    switch (voice->stage) {
        case ENV_IDLE:
            break;
        case ENV_ATTACK:
            voice->envelope += voice->attack_increment;
            if (voice->envelope >= voice->target_amplitude) {
                voice->envelope = voice->target_amplitude;
                voice->stage = ENV_DECAY;
            }
            break;
        case ENV_DECAY: {
            float sustain = voice->sustain_level * voice->target_amplitude;
            voice->envelope -= voice->decay_increment;
            if (voice->envelope <= sustain) {
                voice->envelope = sustain;
                voice->stage = ENV_SUSTAIN;
            }
            break;
        }
        case ENV_SUSTAIN:
            break;
        case ENV_RELEASE:
            voice->envelope -= voice->release_increment;
            if (voice->envelope <= 0.0f) {
                voice->envelope = 0.0f;
                voice->stage = ENV_IDLE;
            }
            break;
    }
}

static void render(Synth *synth, uint32_t start, uint32_t end) {
    float filter_f = minimum(synth->filter_cutoff * 0.5f * PI, 1.5f);
    float filter_q = maximum(1.0f - synth->filter_resonance * 0.24f, 0.05f);

    for (uint32_t frame = start; frame < end; ++frame) {
        float mix = 0.0f;
        for (uint32_t i = 0; i < N_VOICES; ++i) {
            Voice *voice = &synth->voices[i];
            advance_envelope(voice);

            voice->phase += voice->frequency / synth->sample_rate;
            if (voice->phase >= 1.0f) {
                voice->phase -= 1.0f;
            }

            if (voice->envelope > 0.0f) {
                float sample = oscillator_sample(synth->waveform, voice->phase);
                float lowpass = voice->filter_lowpass + filter_f * voice->filter_bandpass;
                float highpass = sample - lowpass - filter_q * voice->filter_bandpass;
                float bandpass = filter_f * highpass + voice->filter_bandpass;
                voice->filter_bandpass = bandpass;
                voice->filter_lowpass = lowpass;
                mix += lowpass * voice->envelope;
            }
        }
        synth->output[frame] = MAX_AMPLITUDE * mix / (float)N_VOICES;
    }
}

static uint32_t pad_size(uint32_t size) {
    return (size + 7u) & ~7u;
}

static LV2_Handle instantiate(const LV2_Descriptor *descriptor, double sample_rate,
                              const char *bundle_path,
                              const LV2_Feature *const *features) {
    (void)descriptor;
    (void)bundle_path;
    const LV2_URID_Map *map = 0;
    if (features) {
        for (uint32_t i = 0; features[i]; ++i) {
            if (strings_equal(features[i]->uri, LV2_URID__map)) {
                map = (const LV2_URID_Map *)features[i]->data;
                break;
            }
        }
    }
    if (!map || !map->map) {
        return 0;
    }

    instance.atom_sequence_urid = map->map(map->handle, LV2_ATOM__Sequence);
    instance.midi_event_urid = map->map(map->handle, LV2_MIDI__MidiEvent);
    if (!instance.atom_sequence_urid || !instance.midi_event_urid) {
        return 0;
    }

    instance.sample_rate = (float)sample_rate;
    instance.midi_in = 0;
    instance.output = 0;
    instance.age_counter = 0;
    instance.waveform = WAVE_SINE;
    instance.attack_seconds = 0.005f;
    instance.decay_seconds = 0.050f;
    instance.sustain_level = 0.2f;
    instance.release_seconds = 0.500f;
    instance.filter_cutoff = 0.5f;
    instance.filter_resonance = 0.5f;
    for (uint32_t i = 0; i < N_VOICES; ++i) {
        reset_voice(&instance.voices[i]);
    }
    return &instance;
}

static void connect_port(LV2_Handle handle, uint32_t port, void *data) {
    Synth *synth = (Synth *)handle;
    if (port == PORT_MIDI_IN) {
        synth->midi_in = (const LV2_Atom_Sequence *)data;
    } else if (port == PORT_OUTPUT) {
        synth->output = (float *)data;
    }
}

static void activate(LV2_Handle handle) {
    Synth *synth = (Synth *)handle;
    for (uint32_t i = 0; i < N_VOICES; ++i) {
        synth->voices[i].gate = 0;
        synth->voices[i].envelope = 0.0f;
        synth->voices[i].stage = ENV_IDLE;
        synth->voices[i].filter_bandpass = 0.0f;
        synth->voices[i].filter_lowpass = 0.0f;
    }
}

static void run(LV2_Handle handle, uint32_t sample_count) {
    Synth *synth = (Synth *)handle;
    uint32_t offset = 0;
    if (synth->midi_in && synth->midi_in->atom.type == synth->atom_sequence_urid &&
        synth->midi_in->atom.size >= sizeof(LV2_Atom_Sequence_Body)) {
        const uint8_t *event_ptr = (const uint8_t *)(&synth->midi_in->body + 1);
        const uint8_t *end = (const uint8_t *)&synth->midi_in->body + synth->midi_in->atom.size;
        while (event_ptr + sizeof(LV2_Atom_Event) <= end) {
            const LV2_Atom_Event *event = (const LV2_Atom_Event *)event_ptr;
            uint32_t event_size = sizeof(LV2_Atom_Event) + pad_size(event->body.size);
            if (event_ptr + event_size > end) {
                break;
            }
            if (event->body.type == synth->midi_event_urid && event->body.size >= 3) {
                uint32_t frame = event->frames < 0 ? 0u : (uint32_t)event->frames;
                frame = frame > sample_count ? sample_count : frame;
                frame = frame < offset ? offset : frame;
                render(synth, offset, frame);
                handle_midi(synth, (const uint8_t *)(event + 1));
                offset = frame;
            }
            event_ptr += event_size;
        }
    }
    render(synth, offset, sample_count);
}

static void deactivate(LV2_Handle handle) {
    (void)handle;
}

static void cleanup(LV2_Handle handle) {
    (void)handle;
}

static const void *extension_data(const char *uri) {
    (void)uri;
    return 0;
}

static const LV2_Descriptor descriptor = {
    PLUGIN_URI, instantiate, connect_port, activate, run, deactivate, cleanup, extension_data,
};

const LV2_Descriptor *lv2_descriptor(uint32_t index) {
    return index == 0 ? &descriptor : 0;
}

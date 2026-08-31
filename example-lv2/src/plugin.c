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

// Port indices for this plugin: a frequency control and one audio output -
// it's a synth (generator), so it has no audio input port.
#define PORT_FREQUENCY 0
#define PORT_OUTPUT 1

#define AMPLITUDE 0.8f

// Freestanding build, no malloc: a single static instance is enough for our
// purposes (we only ever instantiate one copy of this plugin at a time).
typedef struct {
    // Converted from the LV2-mandated `double` once here, rather than kept
    // as a double, so the real-time `run()` path stays on hardware
    // single-precision float instead of libgcc's soft-double routines.
    float sample_rate;
    const float *frequency;
    float *output;
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
    g_state.frequency = 0;
    g_state.output = 0;
    g_state.phase = 0;
    return &g_state;
}

static void connect_port(LV2_Handle instance, uint32_t port, void *data_location) {
    SynthState *synth = (SynthState *)instance;
    switch (port) {
        case PORT_FREQUENCY:
            synth->frequency = (const float *)data_location;
            break;
        case PORT_OUTPUT:
            synth->output = (float *)data_location;
            break;
    }
}

static void activate(LV2_Handle instance) {
    SynthState *synth = (SynthState *)instance;
    synth->phase = 0;
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    SynthState *synth = (SynthState *)instance;
    float freq = (synth->frequency && *synth->frequency > 0.0f) ? *synth->frequency : 440.0f;
    uint32_t period_samples = (uint32_t)(synth->sample_rate / freq);
    if (period_samples < 2) {
        period_samples = 2;
    }
    uint32_t half_period = period_samples / 2;

    for (uint32_t i = 0; i < sample_count; i++) {
        synth->output[i] = (synth->phase < half_period) ? AMPLITUDE : -AMPLITUDE;
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

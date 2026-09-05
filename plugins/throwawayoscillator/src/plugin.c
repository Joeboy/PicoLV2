#include <math.h>
#include <stdint.h>
#include <stdlib.h>

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

#define PORT_FREQUENCY 0
#define PORT_OUTPUT 1

typedef struct {
    float sample_rate;
    float phase;
    const float *frequency;
    float *output;
} Oscillator;

static LV2_Handle instantiate(const LV2_Descriptor *descriptor,
                              double sample_rate,
                              const char *bundle_path,
                              const void *const *features) {
    (void)descriptor;
    (void)bundle_path;
    (void)features;

    Oscillator *oscillator = (Oscillator *)calloc(1, sizeof(Oscillator));
    if (oscillator) {
        oscillator->sample_rate = (float)sample_rate;
    }
    return oscillator;
}

static void connect_port(LV2_Handle instance, uint32_t port, void *data_location) {
    Oscillator *oscillator = (Oscillator *)instance;
    if (port == PORT_FREQUENCY) {
        oscillator->frequency = (const float *)data_location;
    } else if (port == PORT_OUTPUT) {
        oscillator->output = (float *)data_location;
    }
}

static void activate(LV2_Handle instance) {
    ((Oscillator *)instance)->phase = 0.0f;
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    Oscillator *oscillator = (Oscillator *)instance;
    if (!oscillator->output) {
        return;
    }

    const float frequency = oscillator->frequency ? *oscillator->frequency : 0.0f;
    const float phase_increment = frequency / oscillator->sample_rate;
    for (uint32_t frame = 0; frame < sample_count; ++frame) {
        oscillator->output[frame] = sinf(oscillator->phase * 6.28318530718f) * 0.2f;
        oscillator->phase += phase_increment;
        oscillator->phase -= floorf(oscillator->phase);
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
    return NULL;
}

static const LV2_Descriptor descriptor = {
    "https://joebutton.co.uk/lv2/throwawayoscillator",
    instantiate,
    connect_port,
    activate,
    run,
    deactivate,
    cleanup,
    extension_data,
};

const LV2_Descriptor *lv2_descriptor(uint32_t index) {
    return index == 0 ? &descriptor : NULL;
}
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>

typedef void *LV2_Handle;

typedef struct {
    const char *uri;
    void *data;
} LV2_Feature;

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

#define PORT_INPUT  0
#define PORT_OUTPUT 1
#define PORT_GAIN   2

typedef struct {
    const float *input;
    float *output;
    const float *gain;
} AmpInstance;

static LV2_Handle instantiate(const LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const LV2_Feature *const *features) {
    (void)descriptor;
    (void)sample_rate;
    (void)bundle_path;
    (void)features;

    AmpInstance *amp = (AmpInstance *)calloc(1, sizeof(AmpInstance));
    return (LV2_Handle)amp;
}

static void connect_port(LV2_Handle instance, uint32_t port, void *data_location) {
    AmpInstance *amp = (AmpInstance *)instance;
    if (!amp) return;

    switch (port) {
    case PORT_INPUT:
        amp->input = (const float *)data_location;
        break;
    case PORT_OUTPUT:
        amp->output = (float *)data_location;
        break;
    case PORT_GAIN:
        amp->gain = (const float *)data_location;
        break;
    }
}

static void activate(LV2_Handle instance) {
    (void)instance;
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    AmpInstance *amp = (AmpInstance *)instance;
    if (!amp || !amp->input || !amp->output) return;

    float gain = amp->gain ? *amp->gain : 1.0f;
    for (uint32_t i = 0; i < sample_count; i++) {
        amp->output[i] = amp->input[i] * gain;
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

static const LV2_Descriptor g_descriptor = {
    "https://joebutton.co.uk/lv2/amp-poc",
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
        return &g_descriptor;
    }
    return NULL;
}

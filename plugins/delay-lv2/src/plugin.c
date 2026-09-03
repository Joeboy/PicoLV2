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

#define PORT_INPUT      0
#define PORT_OUTPUT     1
#define PORT_DELAY_TIME 2
#define PORT_FEEDBACK   3
#define PORT_DRY_WET    4

#define MAX_DELAY_SAMPLES 24000 // 0.5s @ 48kHz

typedef struct {
    const float *input;
    float *output;
    const float *delay_time;
    const float *feedback;
    const float *dry_wet;

    float sample_rate;
    float ring_buffer[MAX_DELAY_SAMPLES];
    uint32_t write_pos;
} DelayInstance;

static LV2_Handle instantiate(const LV2_Descriptor *descriptor,
                               double sample_rate,
                               const char *bundle_path,
                               const LV2_Feature *const *features) {
    (void)descriptor;
    (void)bundle_path;
    (void)features;

    DelayInstance *delay = (DelayInstance *)calloc(1, sizeof(DelayInstance));
    if (!delay) return NULL;

    delay->sample_rate = (sample_rate > 0.0) ? (float)sample_rate : 48000.0f;
    return (LV2_Handle)delay;
}

static void connect_port(LV2_Handle instance, uint32_t port, void *data_location) {
    DelayInstance *delay = (DelayInstance *)instance;
    if (!delay) return;

    switch (port) {
    case PORT_INPUT:
        delay->input = (const float *)data_location;
        break;
    case PORT_OUTPUT:
        delay->output = (float *)data_location;
        break;
    case PORT_DELAY_TIME:
        delay->delay_time = (const float *)data_location;
        break;
    case PORT_FEEDBACK:
        delay->feedback = (const float *)data_location;
        break;
    case PORT_DRY_WET:
        delay->dry_wet = (const float *)data_location;
        break;
    }
}

static void activate(LV2_Handle instance) {
    (void)instance;
}

static void run(LV2_Handle instance, uint32_t sample_count) {
    DelayInstance *delay = (DelayInstance *)instance;
    if (!delay || !delay->input || !delay->output) return;

    float delay_sec = delay->delay_time ? *delay->delay_time : 0.100f; // 100ms default
    float feedback = delay->feedback ? *delay->feedback : 0.75f; // high feedback for testing
    float mix = delay->dry_wet ? *delay->dry_wet : 0.5f;

    uint32_t delay_samples = (uint32_t)(delay_sec * delay->sample_rate);
    if (delay_samples < 1) delay_samples = 1;
    if (delay_samples >= MAX_DELAY_SAMPLES) delay_samples = MAX_DELAY_SAMPLES - 1;

    for (uint32_t i = 0; i < sample_count; i++) {
        float in = delay->input[i];

        uint32_t read_pos = (delay->write_pos + MAX_DELAY_SAMPLES - delay_samples) % MAX_DELAY_SAMPLES;
        float delayed_sample = delay->ring_buffer[read_pos];

        delay->ring_buffer[delay->write_pos] = in + (delayed_sample * feedback);
        delay->write_pos = (delay->write_pos + 1) % MAX_DELAY_SAMPLES;

        delay->output[i] = in * (1.0f - mix) + delayed_sample * mix;
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
    "https://joebutton.co.uk/lv2/delay-poc",
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

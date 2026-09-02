#include <dlfcn.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Minimal LV2 ABI declarations needed to load and run an instrument. */
typedef struct Feature { const char *uri; void *data; } Feature;
typedef struct UridMap { void *handle; uint32_t (*map)(void *, const char *); } UridMap;
typedef struct Atom { uint32_t size; uint32_t type; } Atom;
typedef struct Sequence { Atom atom; uint32_t unit; uint32_t pad; unsigned char data[64]; } Sequence;
typedef struct Event { int64_t frames; Atom body; unsigned char midi[3]; } Event;
typedef void *(*Instantiate)(const void *, double, const char *, const Feature *const *);
typedef void (*ConnectPort)(void *, uint32_t, void *);
typedef void (*Activate)(void *);
typedef void (*Run)(void *, uint32_t);
typedef struct Descriptor { const char *uri; Instantiate instantiate; ConnectPort connect; Activate activate; Run run; } Descriptor;

static uint32_t map_uri(void *handle, const char *uri) {
    (void)handle;
    if (strcmp(uri, "http://lv2plug.in/ns/ext/atom#Sequence") == 0) return 1;
    if (strcmp(uri, "http://lv2plug.in/ns/ext/midi#MidiEvent") == 0) return 2;
    return 0;
}

/* Build note-on and note-off events in the LV2 Atom Sequence buffer. */
static void set_note(Sequence *sequence, uint32_t midi_urid) {
    Event on = { .frames = 0, .body = { .size = 3, .type = midi_urid }, .midi = { 0x90, 69, 100 } };
    Event off = { .frames = 2400, .body = { .size = 3, .type = midi_urid }, .midi = { 0x80, 69, 0 } };
    sequence->atom.type = 1;
    sequence->atom.size = 8 + sizeof(on) + sizeof(off);
    memcpy(sequence->data, &on, sizeof(on));
    memcpy(sequence->data + sizeof(on), &off, sizeof(off));
}

/* Estimate pitch from the strongest short-lag autocorrelation. */
static double estimate_frequency(const float *samples, size_t count, double sample_rate) {
    double best_score = -INFINITY;
    size_t best_lag = 0;
    for (size_t lag = 96; lag <= 120; ++lag) {
        double score = 0.0;
        for (size_t index = 0; index + lag < count; ++index) {
            score += samples[index] * samples[index + lag];
        }
        if (score > best_score) {
            best_score = score;
            best_lag = lag;
        }
    }
    return best_lag == 0 ? 0.0 : sample_rate / best_lag;
}

int main(int argc, char **argv) {
    /* Pitch checking is optional because complex plugins may not be sinusoidal. */
    int argument = 1;
    int require_release = 0;
    int check_pitch = 0;
    double expected_frequency = 0.0;
    if (argument < argc && strcmp(argv[argument], "--require-release") == 0) {
        require_release = 1;
        argument++;
    }
    if (argument + 2 < argc && strcmp(argv[argument], "--expect-frequency") == 0) {
        check_pitch = 1;
        expected_frequency = strtod(argv[argument + 1], NULL);
        argument += 2;
    }
    const char *plugin_path = argument + 1 == argc ? argv[argument] : NULL;
    if (!plugin_path) return fprintf(stderr, "usage: %s [--require-release] [--expect-frequency hz] plugin.so\n", argv[0]), 2;
    void *library = dlopen(plugin_path, RTLD_NOW);
    if (!library) return fprintf(stderr, "%s\n", dlerror()), 2;
    const Descriptor *(*get_descriptor)(uint32_t) = dlsym(library, "lv2_descriptor");
    if (!get_descriptor) return fprintf(stderr, "lv2_descriptor not found\n"), 2;
    const Descriptor *descriptor = get_descriptor(0);
    if (!descriptor) return fprintf(stderr, "plugin has no descriptor\n"), 2;
    UridMap map = { .map = map_uri };
    Feature map_feature = { "http://lv2plug.in/ns/ext/urid#map", &map };
    const Feature *features[] = { &map_feature, NULL };
    void *instance = descriptor->instantiate(descriptor, 48000.0, NULL, features);
    if (!instance) return fprintf(stderr, "plugin failed to instantiate\n"), 1;
    Sequence sequence = { 0 };
    float output[4800] = { 0 };
    set_note(&sequence, 2);
    descriptor->connect(instance, 0, &sequence);
    descriptor->connect(instance, 1, output);
    descriptor->activate(instance);
    descriptor->run(instance, 4800);

    /* Energy catches silent plugins; autocorrelation provides a lightweight pitch check. */
    double energy = 0.0;
    double first_half_energy = 0.0;
    double second_half_energy = 0.0;
    for (size_t index = 0; index < 4800; ++index) {
        energy += output[index] * output[index];
        if (index < 2400) first_half_energy += output[index] * output[index];
        else second_half_energy += output[index] * output[index];
    }
    double rms = sqrt(energy / 4800.0);
    double first_half_rms = sqrt(first_half_energy / 2400.0);
    double second_half_rms = sqrt(second_half_energy / 2400.0);
    double frequency = estimate_frequency(output, 2400, 48000.0);
    if (rms < 0.001 || first_half_rms < 0.001 || (require_release && second_half_rms < 0.00001) || (check_pitch && (frequency < expected_frequency - 10.0 || frequency > expected_frequency + 10.0))) {
        fprintf(stderr, "unexpected output: rms=%f first_half_rms=%f second_half_rms=%f frequency=%f\n", rms, first_half_rms, second_half_rms, frequency);
        return 1;
    }
    printf("LV2 smoke test: %s, rms=%f", descriptor->uri, rms);
    if (check_pitch) printf(" frequency=%f Hz", frequency);
    putchar('\n');
    dlclose(library);
    return 0;
}
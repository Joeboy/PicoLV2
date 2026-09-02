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
typedef struct Sequence { Atom atom; uint32_t unit; uint32_t pad; unsigned char data[32]; } Sequence;
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

/* Build one timestamp-zero MIDI event in the LV2 Atom Sequence buffer. */
static void set_note(Sequence *sequence, uint32_t midi_urid, unsigned char status) {
    Event event = { .frames = 0, .body = { .size = 3, .type = midi_urid }, .midi = { status, 69, 100 } };
    sequence->atom.type = 1;
    sequence->atom.size = 8 + sizeof(event);
    memcpy(sequence->data, &event, sizeof(event));
}

int main(int argc, char **argv) {
    /* Pitch checking is optional because complex plugins may not be sinusoidal. */
    int check_pitch = argc == 4 && strcmp(argv[1], "--expect-frequency") == 0;
    double expected_frequency = check_pitch ? strtod(argv[2], NULL) : 0.0;
    const char *plugin_path = check_pitch ? NULL : (argc == 2 ? argv[1] : NULL);
    if (check_pitch) plugin_path = argc == 4 ? argv[3] : NULL;
    if (!plugin_path) return fprintf(stderr, "usage: %s [--expect-frequency hz] plugin.so\n", argv[0]), 2;
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
    set_note(&sequence, 2, 0x90);
    descriptor->connect(instance, 0, &sequence);
    descriptor->connect(instance, 1, output);
    descriptor->activate(instance);
    descriptor->run(instance, 4800);

    /* Energy catches silent plugins; zero crossings provide a lightweight pitch check. */
    double energy = 0.0;
    unsigned crossings = 0;
    for (size_t index = 0; index < 4800; ++index) {
        energy += output[index] * output[index];
        if (index > 0 && output[index - 1] <= 0.0f && output[index] > 0.0f) crossings++;
    }
    double rms = sqrt(energy / 4800.0);
    double frequency = crossings * 48000.0 / 4800.0;
    if (rms < 0.001 || (check_pitch && (frequency < expected_frequency - 10.0 || frequency > expected_frequency + 10.0))) {
        fprintf(stderr, "unexpected A4 output: rms=%f crossings=%u frequency=%f\n", rms, crossings, frequency);
        return 1;
    }
    printf("LV2 smoke test: %s, rms=%f", descriptor->uri, rms);
    if (check_pitch) printf(" frequency=%f Hz", frequency);
    putchar('\n');
    dlclose(library);
    return 0;
}
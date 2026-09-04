// malloc/calloc/realloc/free for pico-target plugins, routed through the
// firmware's tracked heap
#include <stddef.h>
#include <string.h>

#define PICO_ALLOC_ALIGN (sizeof(max_align_t))

extern void *picolv2_alloc(size_t size, size_t align);
extern void picolv2_dealloc(void *ptr, size_t size, size_t align);

typedef struct {
    size_t size;
} pico_alloc_header;

void *malloc(size_t size) {
    if (size == 0) {
        size = 1;
    }
    size_t total = size + sizeof(pico_alloc_header);
    if (total < size) {
        return NULL;
    }
    void *raw = picolv2_alloc(total, PICO_ALLOC_ALIGN);
    if (!raw) {
        return NULL;
    }
    ((pico_alloc_header *)raw)->size = total;
    return (char *)raw + sizeof(pico_alloc_header);
}

void free(void *ptr) {
    if (!ptr) {
        return;
    }
    pico_alloc_header *header = (pico_alloc_header *)((char *)ptr - sizeof(pico_alloc_header));
    picolv2_dealloc(header, header->size, PICO_ALLOC_ALIGN);
}

void *calloc(size_t count, size_t size) {
    if (count != 0 && size > (size_t)-1 / count) {
        return NULL;
    }
    size_t total = count * size;
    void *ptr = malloc(total);
    if (ptr) {
        memset(ptr, 0, total);
    }
    return ptr;
}

void *realloc(void *ptr, size_t size) {
    if (!ptr) {
        return malloc(size);
    }
    if (size == 0) {
        free(ptr);
        return NULL;
    }
    pico_alloc_header *header = (pico_alloc_header *)((char *)ptr - sizeof(pico_alloc_header));
    size_t old_size = header->size - sizeof(pico_alloc_header);
    void *new_ptr = malloc(size);
    if (new_ptr) {
        memcpy(new_ptr, ptr, old_size < size ? old_size : size);
        free(ptr);
    }
    return new_ptr;
}

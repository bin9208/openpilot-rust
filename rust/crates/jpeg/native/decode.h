#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
int decode_pixels(const uint8_t *input, size_t input_size, uint64_t max_pixels,
    uint8_t **pixels, size_t *size, unsigned int *width, unsigned int *height,
    unsigned int *components, char *message, size_t message_size);
#ifdef __cplusplus
}
#endif

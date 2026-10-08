#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
int encode_rgb(const uint8_t *rgb,unsigned int width,unsigned int height,unsigned char **output,unsigned long *size,char *error,size_t error_size);
int encode_pixels(const uint8_t *pixels,unsigned int width,unsigned int height,unsigned int components,unsigned int quality,
  unsigned char **output,unsigned long *size,char *error,size_t error_size);
#ifdef __cplusplus
}
#endif

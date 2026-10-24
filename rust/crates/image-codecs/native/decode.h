#pragma once
#include <stddef.h>
#include <stdint.h>
struct image_codec_pixels {
  uint32_t width, height, components, depth;
  int32_t color_space;
  unsigned char *pixels;
  size_t length;
};
int image_codec_avif(const unsigned char *bytes, size_t length,
                     uint64_t max_pixels, struct image_codec_pixels *output,
                     char *message, size_t message_size);
int image_codec_jpeg2000(const unsigned char *bytes, size_t length,
                         uint64_t max_pixels, uint32_t output_depth,
                         int32_t preserve_palette,
                         struct image_codec_pixels *output, char *message,
                         size_t message_size);
void image_codec_pixels_free(struct image_codec_pixels *image);

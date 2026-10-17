#pragma once
#include <stddef.h>
#include <stdint.h>
/* Native codec output is still packed samples, never a universal RGBA image.
 * Rust selects original Pillow mode, sample interpretation and orientation. */
struct image_codec_tiff {
  uint32_t width, height;
  uint16_t bits, channels, sample_format, photometric, compression, orientation,
      fillorder, planar;
  uint16_t extra_count, extras[6], rgba_conversion;
  unsigned char *pixels;
  size_t length, stride;
};
int image_codec_tiff_decode(const unsigned char *, size_t, uint64_t,
                            struct image_codec_tiff *, char *, size_t);
void image_codec_tiff_free(struct image_codec_tiff *);

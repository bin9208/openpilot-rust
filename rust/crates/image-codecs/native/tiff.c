/* Bounded libtiff codec boundary. Policy and integer interpretation live in
 * Rust. The YCbCr path follows Pillow 12.3 TiffDecode.c; Pillow license
 * retained. */
#include "tiff.h"
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tiffio.h>
struct tiff_input {
  const unsigned char *bytes;
  size_t length, position;
  char *error;
  size_t error_size;
  size_t photo_offset;
  int photo_patch, photo_big_endian;
};
static tmsize_t read_input(thandle_t context, void *out, tmsize_t count) {
  struct tiff_input *s = context;
  if (count < 0 || s->position > s->length)
    return -1;
  size_t n = (size_t)count;
  if (n > s->length - s->position)
    n = s->length - s->position;
  if (n)
    memcpy(out, s->bytes + s->position, n);
  if (s->photo_patch) {
    for (unsigned i = 0; i < 2; i++) {
      size_t at = s->photo_offset + i;
      if (at >= s->position && at - s->position < n)
        ((unsigned char *)out)[at - s->position] =
            (unsigned char)(s->photo_big_endian ? (i == 1) : (i == 0));
    }
  }
  s->position += n;
  return (tmsize_t)n;
}
static tmsize_t no_write(thandle_t context, void *b, tmsize_t n) {
  (void)context;
  (void)b;
  (void)n;
  return -1;
}
static toff_t seek_input(thandle_t context, toff_t offset, int whence) {
  struct tiff_input *s = context;
  uint64_t base, at;
  if (whence == SEEK_SET) {
    if (offset > s->length)
      return (toff_t)-1;
    at = offset;
  } else {
    if (whence == SEEK_CUR)
      base = s->position;
    else if (whence == SEEK_END)
      base = s->length;
    else
      return (toff_t)-1;
    if (offset > INT64_MAX) {
      uint64_t backwards = (~offset) + 1;
      if (backwards > base)
        return (toff_t)-1;
      at = base - backwards;
    } else {
      if (base > s->length || offset > s->length - base)
        return (toff_t)-1;
      at = base + offset;
    }
  }
  s->position = (size_t)at;
  return at;
}
static int no_close(thandle_t context) {
  (void)context;
  return 0;
}
static toff_t input_size(thandle_t context) {
  return ((struct tiff_input *)context)->length;
}
static int no_map(thandle_t context, void **base, toff_t *size) {
  (void)context;
  (void)base;
  (void)size;
  return 0;
}
static void no_unmap(thandle_t context, void *base, toff_t size) {
  (void)context;
  (void)base;
  (void)size;
}
static int error_handler(TIFF *tiff, void *context, const char *module,
                         const char *format, va_list args) {
  (void)tiff;
  (void)module;
  struct tiff_input *s = context;
  if (s->error_size)
    vsnprintf(s->error, s->error_size, format, args);
  return 1;
}
static int warning_handler(TIFF *tiff, void *context, const char *module,
                           const char *format, va_list args) {
  (void)tiff;
  (void)context;
  (void)module;
  (void)format;
  (void)args;
  return 1;
}
static void failure(struct tiff_input *s, const char *message) {
  if (s->error_size)
    snprintf(s->error, s->error_size, "%s", message);
}
void image_codec_tiff_free(struct image_codec_tiff *image) {
  free(image->pixels);
  memset(image, 0, sizeof(*image));
}
static void copy_bits(unsigned char *out, size_t out_bit,
                      const unsigned char *in, size_t in_bit, size_t count) {
  for (size_t bit = 0; bit < count; ++bit) {
    unsigned char mask = (unsigned char)(1u << (7 - (out_bit + bit) % 8));
    unsigned char v =
        (unsigned char)((in[(in_bit + bit) / 8] >> (7 - (in_bit + bit) % 8)) &
                        1u);
    out[(out_bit + bit) / 8] =
        (unsigned char)((out[(out_bit + bit) / 8] & ~mask) | (v ? mask : 0));
  }
}
int image_codec_tiff_decode(const unsigned char *bytes, size_t length,
                            uint64_t limit, struct image_codec_tiff *out,
                            char *message, size_t message_size) {
  memset(out, 0, sizeof(*out));
  struct tiff_input input = {bytes, length, 0, message, message_size, 0, 0, 0};
  TIFF *tiff = NULL;
  TIFFOpenOptions *options = TIFFOpenOptionsAlloc();
  unsigned char *block = NULL;
  int ok = 0;
  if (!options) {
    failure(&input, "TIFF options allocation failed");
    goto done;
  }
  /* Up to six 32-bit channels plus packed-row padding. This bounds both our
   * scratch blocks and libtiff's internal allocations by the caller's admitted
   * pixel budget rather than trusting attacker-supplied tile dimensions. */
  if (!limit) {
    failure(&input, "TIFF zero pixel budget");
    goto done;
  }
  uint64_t allocation_limit =
      limit > (uint64_t)PTRDIFF_MAX / 25 ? (uint64_t)PTRDIFF_MAX : limit * 25;
  /* A fixed 1 MiB allowance covers directory tables and normal padded tiles
   * when callers deliberately use a tiny image-pixel limit. */
  if (allocation_limit < 1024 * 1024)
    allocation_limit = 1024 * 1024;
  TIFFOpenOptionsSetMaxSingleMemAlloc(options, (tmsize_t)allocation_limit);
  TIFFOpenOptionsSetMaxCumulatedMemAlloc(options, (tmsize_t)allocation_limit);
  TIFFOpenOptionsSetErrorHandlerExtR(options, error_handler, &input);
  TIFFOpenOptionsSetWarningHandlerExtR(options, warning_handler, &input);
  /* LibTIFF refuses a one-channel YCbCr directory, while Pillow's raw
   * reader accepts its L samples. Change only this photometric metadata in
   * a two-byte callback overlay; it is equivalent to the original raw L
   * unpacker and never allocates a second copy of the encoded input. */
  if (length >= 8 && ((bytes[0] == 'I' && bytes[1] == 'I' && bytes[2] == 42 &&
                       bytes[3] == 0) ||
                      (bytes[0] == 'M' && bytes[1] == 'M' && bytes[2] == 0 &&
                       bytes[3] == 42))) {
    int be = bytes[0] == 'M';
    uint32_t directory = 0;
    for (unsigned i = 0; i < 4; i++)
      directory |= (uint32_t)bytes[4 + i] << (be ? (3 - i) * 8 : i * 8);
    if (directory <= length && length - directory >= 2) {
      unsigned count =
          be ? ((unsigned)bytes[directory] << 8) | bytes[directory + 1]
             : bytes[directory] | ((unsigned)bytes[directory + 1] << 8);
      unsigned channels = 1, compression = 1;
      size_t photo = 0;
      if (count <= (length - directory - 2) / 12) {
        for (unsigned i = 0; i < count; i++) {
          size_t at = directory + 2 + (size_t)i * 12;
          unsigned tag = be ? ((unsigned)bytes[at] << 8) | bytes[at + 1]
                            : bytes[at] | ((unsigned)bytes[at + 1] << 8);
          unsigned kind = be ? ((unsigned)bytes[at + 2] << 8) | bytes[at + 3]
                             : bytes[at + 2] | ((unsigned)bytes[at + 3] << 8);
          uint32_t n = 0;
          for (unsigned j = 0; j < 4; j++)
            n |= (uint32_t)bytes[at + 4 + j] << (be ? (3 - j) * 8 : j * 8);
          if (kind != 3 || n != 1)
            continue;
          unsigned value = be ? ((unsigned)bytes[at + 8] << 8) | bytes[at + 9]
                              : bytes[at + 8] | ((unsigned)bytes[at + 9] << 8);
          if (tag == 277)
            channels = value;
          if (tag == 259)
            compression = value;
          if (tag == 262 && value == 6)
            photo = at + 8;
        }
        if (photo && channels == 1 && compression == 1) {
          input.photo_offset = photo;
          input.photo_patch = 1;
          input.photo_big_endian = be;
        }
      }
    }
  }
  tiff = TIFFClientOpenExt("memory", "rm", &input, read_input, no_write,
                           seek_input, no_close, input_size, no_map, no_unmap,
                           options);
  TIFFOpenOptionsFree(options);
  options = NULL;
  if (!tiff)
    goto done;
  if (!TIFFGetField(tiff, TIFFTAG_IMAGEWIDTH, &out->width) ||
      !TIFFGetField(tiff, TIFFTAG_IMAGELENGTH, &out->height) || !out->width ||
      !out->height || (uint64_t)out->width * out->height > limit) {
    failure(&input, "TIFF dimensions invalid or exceed pixel limit");
    goto done;
  }
  uint16_t planar = 1;
  TIFFGetFieldDefaulted(tiff, TIFFTAG_BITSPERSAMPLE, &out->bits);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_SAMPLESPERPIXEL, &out->channels);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_SAMPLEFORMAT, &out->sample_format);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_PHOTOMETRIC, &out->photometric);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_COMPRESSION, &out->compression);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_PLANARCONFIG, &planar);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_ORIENTATION, &out->orientation);
  TIFFGetFieldDefaulted(tiff, TIFFTAG_FILLORDER, &out->fillorder);
  out->planar = planar;
  uint16_t *extras = NULL;
  TIFFGetFieldDefaulted(tiff, TIFFTAG_EXTRASAMPLES, &out->extra_count, &extras);
  if (out->channels < 1 || out->channels > 6 || out->bits < 1 ||
      out->bits > 32 || out->extra_count > 6 || planar < 1 || planar > 2) {
    failure(&input, "TIFF sample layout unsupported");
    goto done;
  }
  for (uint16_t i = 0; i < out->extra_count; ++i)
    out->extras[i] = extras[i];
  if (out->photometric == PHOTOMETRIC_YCBCR &&
      out->compression == COMPRESSION_NONE && !TIFFIsTiled(tiff) &&
      out->channels == 3) {
    /* Pillow's uncompressed YCbCr path uses raw RGBX without conversion.
     * It reads four bytes per pixel from each strip offset, ignoring
     * StripByteCounts. Preserve this behavior, including accepting bytes
     * following a short strip. */
    uint64_t *offsets = NULL;
    uint32_t rows_per_strip = 0;
    TIFFGetFieldDefaulted(tiff, TIFFTAG_ROWSPERSTRIP, &rows_per_strip);
    if (!rows_per_strip ||
        !TIFFGetField(tiff, TIFFTAG_STRIPOFFSETS, &offsets) ||
        out->width > SIZE_MAX / 4 / out->height)
      goto done;
    out->stride = (size_t)out->width * 4;
    out->length = out->stride * out->height;
    out->pixels = malloc(out->length);
    if (!out->pixels)
      goto done;
    for (uint32_t y = 0; y < out->height;) {
      uint32_t rows =
          rows_per_strip < out->height - y ? rows_per_strip : out->height - y;
      uint32_t strip = TIFFComputeStrip(tiff, y, 0);
      if (strip >= TIFFNumberOfStrips(tiff))
        goto done;
      uint64_t at = offsets[strip];
      size_t n = (size_t)rows * out->stride;
      if (at > length || n > length - at)
        goto done;
      memcpy(out->pixels + (size_t)y * out->stride, bytes + (size_t)at, n);
      y += rows;
    }
    out->rgba_conversion = 1;
    ok = 1;
    goto done;
  }
  if (out->photometric == PHOTOMETRIC_YCBCR) {
    if (out->compression == COMPRESSION_JPEG && planar == PLANARCONFIG_CONTIG) {
      if (!TIFFSetField(tiff, TIFFTAG_JPEGCOLORMODE, JPEGCOLORMODE_RGB))
        goto done;
    } else if (out->compression != COMPRESSION_NONE) {
      /* This conversion is specific to the original YCbCr path. Other modes,
       * including palette and CMYK, retain their original samples. */
      if (out->width > SIZE_MAX / 4 / out->height) {
        failure(&input, "TIFF RGBA size overflow");
        goto done;
      }
      size_t n = (size_t)out->width * out->height * 4;
      out->pixels = malloc(n);
      if (!out->pixels)
        goto done;
      TIFFRGBAImage image;
      char error[1024] = {0};
      if (!TIFFRGBAImageOK(tiff, error) ||
          !TIFFRGBAImageBegin(&image, tiff, 0, error)) {
        failure(&input, error);
        goto done;
      }
      image.orientation = ORIENTATION_TOPLEFT;
      image.req_orientation = ORIENTATION_TOPLEFT;
      image.col_offset = 0;
      image.row_offset = 0;
      int got = TIFFRGBAImageGet(&image, (uint32_t *)out->pixels, out->width,
                                 out->height);
      TIFFRGBAImageEnd(&image);
      if (!got)
        goto done;
      out->rgba_conversion = 1;
      out->length = n;
      out->stride = (size_t)out->width * 4;
      ok = 1;
      goto done;
    }
  }
  uint64_t rowbits = (uint64_t)out->width * out->channels * out->bits;
  uint64_t row = (rowbits + 7) / 8;
  if (row > SIZE_MAX / out->height) {
    failure(&input, "TIFF sample size overflow");
    goto done;
  }
  out->stride = (size_t)row;
  out->length = out->stride * out->height;
  out->pixels = calloc(out->length, 1);
  if (!out->pixels) {
    failure(&input, "TIFF sample allocation failed");
    goto done;
  }
  unsigned planes = planar == PLANARCONFIG_SEPARATE ? out->channels : 1;
  unsigned plane_channels = planes == 1 ? out->channels : 1;
  if (TIFFIsTiled(tiff)) {
    uint32_t tw = 0, th = 0;
    if (!TIFFGetField(tiff, TIFFTAG_TILEWIDTH, &tw) ||
        !TIFFGetField(tiff, TIFFTAG_TILELENGTH, &th) || !tw || !th)
      goto done;
    uint64_t size = TIFFTileSize64(tiff), stride = TIFFTileRowSize64(tiff);
    if (!size || size > allocation_limit || !stride || stride > size ||
        stride > allocation_limit / th || stride * th > size) {
      failure(&input, "TIFF tile exceeds admitted allocation budget");
      goto done;
    }
    block = malloc((size_t)size);
    if (!block)
      goto done;
    for (unsigned plane = 0; plane < planes; ++plane)
      for (uint64_t y = 0; y < out->height; y += th)
        for (uint64_t x = 0; x < out->width; x += tw) {
          tmsize_t n = TIFFReadTile(tiff, block, x, y, 0, (uint16_t)plane);
          if (n < 0)
            goto done;
          uint32_t rows = th < out->height - y ? th : out->height - y,
                   cols = tw < out->width - x ? tw : out->width - x;
          for (uint32_t dy = 0; dy < rows; ++dy) {
            if ((uint64_t)dy * stride +
                    ((uint64_t)cols * plane_channels * out->bits + 7) / 8 >
                (uint64_t)n)
              goto done;
            for (uint32_t dx = 0; dx < cols; ++dx)
              for (unsigned c = 0; c < plane_channels; ++c)
                copy_bits(out->pixels + (size_t)(y + dy) * out->stride,
                          ((size_t)(x + dx) * out->channels +
                           (planes == 1 ? c : plane)) *
                              out->bits,
                          block + (size_t)dy * stride,
                          ((size_t)dx * plane_channels + c) * out->bits,
                          out->bits);
          }
        }
  } else {
    uint32_t rows_per_strip = 0;
    TIFFGetFieldDefaulted(tiff, TIFFTAG_ROWSPERSTRIP, &rows_per_strip);
    if (!rows_per_strip)
      goto done;
    uint64_t size = TIFFStripSize64(tiff), stride = TIFFScanlineSize64(tiff);
    uint64_t rows = rows_per_strip < out->height ? rows_per_strip : out->height;
    if (!size || size > allocation_limit || !stride || stride > out->stride ||
        stride > allocation_limit / rows || stride * rows > size) {
      failure(&input, "TIFF strip exceeds admitted allocation budget");
      goto done;
    }
    block = malloc((size_t)size);
    if (!block)
      goto done;
    for (unsigned plane = 0; plane < planes; ++plane)
      for (uint32_t y = 0; y < out->height;) {
        uint32_t rows =
            rows_per_strip < out->height - y ? rows_per_strip : out->height - y;
        tmsize_t n = TIFFReadEncodedStrip(
            tiff, TIFFComputeStrip(tiff, y, (uint16_t)plane), block,
            (tmsize_t)size);
        if (n < 0)
          goto done;
        for (uint32_t dy = 0; dy < rows; ++dy) {
          if ((uint64_t)dy * stride +
                  ((uint64_t)out->width * plane_channels * out->bits + 7) / 8 >
              (uint64_t)n)
            goto done;
          for (uint32_t x = 0; x < out->width; ++x)
            for (unsigned c = 0; c < plane_channels; ++c)
              copy_bits(
                  out->pixels + (size_t)(y + dy) * out->stride,
                  ((size_t)x * out->channels + (planes == 1 ? c : plane)) *
                      out->bits,
                  block + (size_t)dy * stride,
                  ((size_t)x * plane_channels + c) * out->bits, out->bits);
        }
        y += rows;
      }
  }
  ok = 1;
done:
  free(block);
  if (options)
    TIFFOpenOptionsFree(options);
  if (tiff)
    TIFFClose(tiff);
  if (!ok)
    image_codec_tiff_free(out);
  return ok;
}

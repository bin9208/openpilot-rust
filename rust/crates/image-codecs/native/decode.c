/* Decoder-only C ABI. All codec resources and failure cleanup stay inside C;
 * Rust receives one owned packed pixel allocation and copies it fallibly. */
#include "decode.h"
#include <avif/avif.h>
#include <openjpeg.h>
#include <stdlib.h>
#include <string.h>
static void error_text(char *output, size_t size, const char *message) {
  if (size) {
    strncpy(output, message, size - 1);
    output[size - 1] = '\0';
  }
}
void image_codec_pixels_free(struct image_codec_pixels *image) {
  free(image->pixels);
  memset(image, 0, sizeof(*image));
}
int image_codec_avif(const unsigned char *bytes, size_t length,
                     uint64_t max_pixels, struct image_codec_pixels *output,
                     char *message, size_t message_size) {
  memset(output, 0, sizeof(*output));
  avifDecoder *decoder = avifDecoderCreate();
  if (!decoder) {
    error_text(message, message_size, "AVIF decoder allocation failed");
    return 0;
  }
  decoder->maxThreads = 2;
  decoder->strictFlags &= ~(AVIF_STRICT_CLAP_VALID | AVIF_STRICT_PIXI_REQUIRED);
  avifRGBImage rgb = {0};
  int allocated = 0, ok = 0;
  avifResult result = avifDecoderSetIOMemory(decoder, bytes, length);
  if (result != AVIF_RESULT_OK)
    goto done;
  result = avifDecoderParse(decoder);
  if (result != AVIF_RESULT_OK)
    goto done;
  if (!decoder->image->width || !decoder->image->height ||
      (uint64_t)decoder->image->width * decoder->image->height > max_pixels) {
    error_text(message, message_size, "AVIF pixel limit exceeded");
    goto done;
  }
  result = avifDecoderNthImage(decoder, 0);
  if (result != AVIF_RESULT_OK)
    goto done;
  avifRGBImageSetDefaults(&rgb, decoder->image);
  rgb.depth = 8;
  uint32_t components = decoder->alphaPresent ? 4
                        : decoder->image->yuvFormat == AVIF_PIXEL_FORMAT_YUV400
                            ? 1
                            : 3;
  rgb.format = components == 4   ? AVIF_RGB_FORMAT_RGBA
               : components == 1 ? AVIF_RGB_FORMAT_GRAY
                                 : AVIF_RGB_FORMAT_RGB;
  result = avifRGBImageAllocatePixels(&rgb);
  if (result != AVIF_RESULT_OK)
    goto done;
  allocated = 1;
  result = avifImageYUVToRGB(decoder->image, &rgb);
  if (result != AVIF_RESULT_OK)
    goto done;
  if (rgb.height && rgb.rowBytes > SIZE_MAX / rgb.height) {
    error_text(message, message_size, "AVIF size overflow");
    goto done;
  }
  size_t size = (size_t)rgb.rowBytes * rgb.height;
  output->pixels = malloc(size);
  if (!output->pixels) {
    error_text(message, message_size, "AVIF pixel allocation failed");
    goto done;
  }
  memcpy(output->pixels, rgb.pixels, size);
  output->length = size;
  output->width = rgb.width;
  output->height = rgb.height;
  output->components = components;
  output->depth = 8;
  ok = 1;
done:
  if (!ok && result != AVIF_RESULT_OK)
    error_text(message, message_size, avifResultToString(result));
  if (allocated)
    avifRGBImageFreePixels(&rgb);
  avifDecoderDestroy(decoder);
  return ok;
}
struct memory_input {
  const unsigned char *bytes;
  size_t length, position;
};
static OPJ_SIZE_T input_read(void *output, OPJ_SIZE_T count, void *context) {
  struct memory_input *input = context;
  size_t left = input->length - input->position;
  if (!left)
    return (OPJ_SIZE_T)-1;
  if (count > left)
    count = left;
  memcpy(output, input->bytes + input->position, count);
  input->position += count;
  return count;
}
static OPJ_OFF_T input_skip(OPJ_OFF_T count, void *context) {
  struct memory_input *input = context;
  if (count < 0 || (uint64_t)count > input->length - input->position)
    return -1;
  input->position += (size_t)count;
  return count;
}
static OPJ_BOOL input_seek(OPJ_OFF_T position, void *context) {
  struct memory_input *input = context;
  if (position < 0 || (uint64_t)position > input->length)
    return OPJ_FALSE;
  input->position = (size_t)position;
  return OPJ_TRUE;
}
struct diagnostic {
  char *message;
  size_t size;
};
static void diagnostic_error(const char *message, void *context) {
  struct diagnostic *diagnostic = context;
  error_text(diagnostic->message, diagnostic->size, message);
}
static void diagnostic_silent(const char *message, void *context) {
  (void)message;
  (void)context;
}
int image_codec_jpeg2000(const unsigned char *bytes, size_t length,
                         uint64_t max_pixels, uint32_t output_depth,
                         int32_t preserve_palette,
                         struct image_codec_pixels *output, char *message,
                         size_t message_size) {
  memset(output, 0, sizeof(*output));
  int ok = 0;
  opj_image_t *image = NULL;
  opj_stream_t *stream = NULL;
  opj_codec_t *codec = NULL;
  if (length < 4 || (output_depth != 8 && output_depth != 16)) {
    error_text(message, message_size,
               "JPEG2000 input or output precision invalid");
    return 0;
  }
  struct memory_input input = {bytes, length, 0};
  struct diagnostic diagnostic = {message, message_size};
  stream = opj_stream_create(65536, OPJ_TRUE);
  codec =
      opj_create_decompress(bytes[0] == 255 ? OPJ_CODEC_J2K : OPJ_CODEC_JP2);
  if (!stream || !codec) {
    error_text(message, message_size, "JPEG2000 decoder allocation failed");
    goto done;
  }
  opj_stream_set_user_data(stream, &input, NULL);
  opj_stream_set_user_data_length(stream, length);
  opj_stream_set_read_function(stream, input_read);
  opj_stream_set_skip_function(stream, input_skip);
  opj_stream_set_seek_function(stream, input_seek);
  opj_set_error_handler(codec, diagnostic_error, &diagnostic);
  opj_set_warning_handler(codec, diagnostic_silent, NULL);
  opj_set_info_handler(codec, diagnostic_silent, NULL);
  opj_dparameters_t parameters;
  opj_set_default_decoder_parameters(&parameters);
  if (preserve_palette)
    parameters.flags |= OPJ_DPARAMETERS_IGNORE_PCLR_CMAP_CDEF_FLAG;
  if (!opj_setup_decoder(codec, &parameters) ||
      !opj_read_header(stream, codec, &image))
    goto done;
  uint32_t width = image->x1 - image->x0, height = image->y1 - image->y0,
           components = image->numcomps;
  if (image->color_space == OPJ_CLRSPC_UNKNOWN ||
      image->color_space == OPJ_CLRSPC_UNSPECIFIED) {
    if (components <= 2)
      image->color_space = OPJ_CLRSPC_GRAY;
    else {
      int subsampling = -1;
      for (uint32_t c = 0; c < components; ++c) {
        if (image->comps[c].dx != 1 || image->comps[c].dy != 1) {
          subsampling = (int)c;
          break;
        }
      }
      image->color_space = (subsampling == 1 || subsampling == 2)
                               ? OPJ_CLRSPC_SYCC
                               : OPJ_CLRSPC_SRGB;
    }
  }
  if (!width || !height || (uint64_t)width * height > max_pixels ||
      components < 1 || components > 4) {
    error_text(message, message_size,
               "JPEG2000 dimensions or components invalid");
    goto done;
  }
  if (!opj_decode(codec, stream, image) || !opj_end_decompress(codec, stream))
    goto done;
  if ((uint64_t)width * height > SIZE_MAX / components / (output_depth / 8)) {
    error_text(message, message_size, "JPEG2000 pixel size overflow");
    goto done;
  }
  size_t size = (size_t)width * height * components * (output_depth / 8);
  output->pixels = malloc(size);
  if (!output->pixels) {
    error_text(message, message_size, "JPEG2000 pixel allocation failed");
    goto done;
  }
  for (uint32_t y = 0; y < height; ++y)
    for (uint32_t x = 0; x < width; ++x)
      for (uint32_t c = 0; c < components; ++c) {
        opj_image_comp_t *plane = &image->comps[c];
        if (!plane->data || !plane->dx || !plane->dy || !plane->prec ||
            plane->prec > 31) {
          error_text(message, message_size,
                     "JPEG2000 component layout invalid");
          goto done;
        }
        uint32_t px = x / plane->dx, py = y / plane->dy;
        if (px >= plane->w || py >= plane->h) {
          error_text(message, message_size, "JPEG2000 component grid mismatch");
          goto done;
        }
        int shift = (int)output_depth - (int)plane->prec;
        int64_t offset = plane->sgnd ? (int64_t)1 << (plane->prec - 1) : 0;
        if (shift < 0)
          offset += (int64_t)1 << (-shift - 1);
        uint64_t value =
            (uint64_t)((int64_t)plane->data[(size_t)py * plane->w + px] +
                       offset);
        value = shift < 0 ? value >> -shift : value << shift;
        size_t at = ((size_t)y * width + x) * components + c;
        at *= output_depth / 8;
        output->pixels[at] = (uint8_t)value;
        if (output_depth == 16)
          output->pixels[at + 1] = (uint8_t)(value >> 8);
      }
  output->width = width;
  output->height = height;
  output->components = components;
  output->depth = output_depth;
  output->color_space = image->color_space;
  output->length = size;
  ok = 1;
done:
  if (!ok)
    image_codec_pixels_free(output);
  if (image)
    opj_image_destroy(image);
  if (codec)
    opj_destroy_codec(codec);
  if (stream)
    opj_stream_destroy(stream);
  return ok;
}

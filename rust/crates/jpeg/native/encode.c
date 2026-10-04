#include "encode.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include "jpeglib.h"
struct EncoderError { struct jpeg_error_mgr manager; jmp_buf jump; char message[JMSG_LENGTH_MAX]; };
struct Encoder { struct jpeg_compress_struct info; struct EncoderError error; };
static void fail(j_common_ptr info) {
  struct EncoderError *error=(struct EncoderError *)info->err;
  (*info->err->format_message)(info,error->message);
  longjmp(error->jump,1);
}
int encode_pixels(const uint8_t *pixels,unsigned int width,unsigned int height,unsigned int components,unsigned int quality,
    unsigned char **output,unsigned long *size,char *message,size_t message_size) {
  struct Encoder *state=calloc(1,sizeof(*state));
  if (!state) {
    if (message_size) { strncpy(message,"JPEG allocation failed",message_size-1); message[message_size-1]='\0'; }
    return 0;
  }
  state->info.err=jpeg_std_error(&state->error.manager);
  state->error.manager.error_exit=fail;
  if (setjmp(state->error.jump)) {
    jpeg_destroy_compress(&state->info);
    free(*output); *output=NULL; *size=0;
    if (message_size) { strncpy(message,state->error.message,message_size-1); message[message_size-1]='\0'; }
    free(state);
    return 0;
  }
  jpeg_create_compress(&state->info);
  jpeg_mem_dest(&state->info,output,size);
  state->info.image_width=width; state->info.image_height=height;
  state->info.input_components=(int)components; state->info.in_color_space=components==1 ? JCS_GRAYSCALE : JCS_RGB;
  jpeg_set_defaults(&state->info);
  jpeg_set_quality(&state->info,(int)quality,TRUE);
  state->info.dct_method=JDCT_ISLOW;
  jpeg_start_compress(&state->info,TRUE);
  while (state->info.next_scanline<state->info.image_height) {
    JSAMPROW row=(JSAMPROW)(pixels+(size_t)state->info.next_scanline*width*components);
    jpeg_write_scanlines(&state->info,&row,1);
  }
  jpeg_finish_compress(&state->info);
  jpeg_destroy_compress(&state->info);
  free(state);
  return 1;
}
int encode_rgb(const uint8_t *rgb,unsigned int width,unsigned int height,unsigned char **output,unsigned long *size,char *message,size_t message_size) {
  return encode_pixels(rgb,width,height,3,75,output,size,message,message_size);
}

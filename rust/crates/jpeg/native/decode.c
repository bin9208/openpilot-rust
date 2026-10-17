/* libjpeg recovery remains entirely inside C. No Rust/C++ stack is unwound by
 * longjmp. Mutable cleanup state lives on the heap across setjmp. */
#include "decode.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <setjmp.h>
#include "jpeglib.h"
#include "jerror.h"
struct DecodeError { struct jpeg_error_mgr manager; jmp_buf jump; char message[JMSG_LENGTH_MAX]; };
struct Decoder { struct jpeg_decompress_struct info; struct DecodeError error; uint8_t *pixels; };
static void fail(j_common_ptr info) {
  struct DecodeError *error=(struct DecodeError *)info->err;
  (*info->err->format_message)(info,error->message);
  longjmp(error->jump,1);
}
static void warning(j_common_ptr info,int level) {
  /* Pillow does not synthesize an EOI marker for an incomplete stream. Other
   * recoverable libjpeg warnings remain recoverable, as in Pillow's decoder. */
  if (level<0 && info->err->msg_code==JWRN_JPEG_EOF) fail(info);
}
static void message(char *out,size_t size,const char *value) {
  if (size) { strncpy(out,value,size-1); out[size-1]='\0'; }
}
int decode_pixels(const uint8_t *input,size_t input_size,uint64_t max_pixels,
    uint8_t **pixels,size_t *size,unsigned int *width,unsigned int *height,
    unsigned int *components,char *error,size_t error_size) {
  *pixels=NULL; *size=0;
  if (!input_size || input_size>ULONG_MAX) { message(error,error_size,"invalid JPEG input size"); return 0; }
  struct Decoder *state=calloc(1,sizeof(*state));
  if (!state) { message(error,error_size,"JPEG allocation failed"); return 0; }
  state->info.err=jpeg_std_error(&state->error.manager);
  state->error.manager.error_exit=fail; state->error.manager.emit_message=warning;
  if (setjmp(state->error.jump)) {
    /* jpeg_CreateDecompress can fail after initializing its memory manager.
     * jpeg_destroy_decompress explicitly accepts an uninitialized NULL mem. */
    jpeg_destroy_decompress(&state->info);
    free(state->pixels); message(error,error_size,state->error.message); free(state); return 0;
  }
  jpeg_create_decompress(&state->info);
  jpeg_mem_src(&state->info,input,(unsigned long)input_size);
  jpeg_read_header(&state->info,TRUE);
  if (!state->info.image_width || !state->info.image_height ||
      (uint64_t)state->info.image_width*state->info.image_height>max_pixels) {
    message(state->error.message,sizeof(state->error.message),"JPEG image exceeds pixel limit");
    longjmp(state->error.jump,1);
  }
  int cmyk=state->info.jpeg_color_space==JCS_CMYK || state->info.jpeg_color_space==JCS_YCCK;
  state->info.out_color_space=cmyk?JCS_CMYK:state->info.num_components==1?JCS_GRAYSCALE:JCS_RGB;
  state->info.dct_method=JDCT_ISLOW; state->info.do_fancy_upsampling=TRUE;
  jpeg_start_decompress(&state->info);
  size_t stride=(size_t)state->info.output_width*state->info.output_components;
  if (stride>SIZE_MAX/state->info.output_height) {
    message(state->error.message,sizeof(state->error.message),"JPEG image size overflow"); longjmp(state->error.jump,1);
  }
  size_t count=stride*state->info.output_height;
  state->pixels=malloc(count);
  if (!state->pixels) { message(state->error.message,sizeof(state->error.message),"JPEG allocation failed"); longjmp(state->error.jump,1); }
  while (state->info.output_scanline<state->info.output_height) {
    JSAMPROW row=state->pixels+stride*state->info.output_scanline;
    if (jpeg_read_scanlines(&state->info,&row,1)!=1) {
      message(state->error.message,sizeof(state->error.message),"JPEG short scanline"); longjmp(state->error.jump,1);
    }
  }
  *width=state->info.output_width; *height=state->info.output_height; *components=state->info.output_components;
  jpeg_finish_decompress(&state->info); jpeg_destroy_decompress(&state->info);
  /* Pillow's CMYK;I rawmode inverts all four channels, including non-Adobe JPEG. */
  if (cmyk) { for (size_t i=0;i<count;++i) state->pixels[i]=255-state->pixels[i]; }
  *pixels=state->pixels; *size=count; free(state); return 1;
}

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <jpeglib.h>
#include "decode.h"
static int calls;
void *__real_jpeg_get_small(j_common_ptr info,size_t size);
void *__wrap_jpeg_get_small(j_common_ptr info,size_t size) {
  if (++calls>=2) return NULL;
  return __real_jpeg_get_small(info,size);
}
int main(void) {
  uint8_t input[]={0xff,0xd8,0xff,0xd9};
  uint8_t *pixels=NULL;size_t size=0;unsigned int width=0,height=0,components=0;char error[256]={0};
  int ok=decode_pixels(input,sizeof(input),178956970,&pixels,&size,&width,&height,&components,error,sizeof(error));
  free(pixels);
  if (ok || calls<2) return 2;
  puts(error);return 0;
}

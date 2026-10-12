#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int framebuffer=19, buffer=31, alignment=8;
static unsigned int error_code, next_buffer=100, next_fence=1000;
static size_t sizes[256];
static unsigned char bytes[256][4096], active[256], fences[256];
static int mapped, unmap_calls;
static int mode(const char *name) { const char *value=getenv("OWNED_GL_SCENARIO");return value && strcmp(value,name)==0; }
static void summary(void) {
  unsigned int buffers=0, syncs=0;
  for(unsigned int i=0;i<256;++i){buffers+=active[i]!=0;syncs+=fences[i]!=0;}
  printf("{\"provider_summary\":{\"buffers\":%u,\"fences\":%u,\"mapped\":%d,\"bindings\":[%d,%d,%d]}}\n",buffers,syncs,mapped,framebuffer,buffer,alignment);
}
__attribute__((constructor)) static void initialize(void) {sizes[31]=16;atexit(summary);}
void glGetIntegerv(unsigned int name,int *value) {
  switch(name){case 0x8CA6:*value=framebuffer;break;case 0x88ED:*value=buffer;break;case 0x0D05:*value=alignment;break;default:*value=0;}
}
unsigned int glGetError(void) {if(mode("uncleared"))return 0x0500;unsigned int value=error_code;error_code=0;return value;}
void glBindFramebuffer(unsigned int target,unsigned int name){(void)target;framebuffer=(int)name;}
void glBindBuffer(unsigned int target,unsigned int name){(void)target;buffer=(int)name;}
void glPixelStorei(unsigned int name,int value){if(name==0x0D05)alignment=value;}
void glReadPixels(int x,int y,int width,int height,unsigned int format,unsigned int type,void *destination) {
  (void)x;(void)y;(void)format;(void)type;
  unsigned char *output=destination ? destination : bytes[buffer];
  for(int i=0;i<width*height*4;++i)output[i]=(unsigned char)i;
  if((destination && mode("direct-error")) || (!destination && mode("read-error")))error_code=0x0502;
}
void glGenBuffers(int count,unsigned int *names) {
  for(int i=0;i<count;++i){names[i]=mode("allocation-zero") && i==1 ? 0 : ++next_buffer;if(names[i])active[names[i]]=1;}
}
void glDeleteBuffers(int count,const unsigned int *names){for(int i=0;i<count;++i)if(names[i]<256)active[names[i]]=0;}
void glBufferData(unsigned int target,ptrdiff_t size,const void *data,unsigned int usage) {
  (void)target;(void)data;(void)usage;sizes[buffer]=(size_t)size;
  if(mode("allocation-error"))error_code=0x0505;
}
void glGetBufferParameteriv(unsigned int target,unsigned int name,int *value){(void)target;(void)name;*value=(int)sizes[buffer];}
void *glMapBufferRange(unsigned int target,ptrdiff_t offset,ptrdiff_t length,unsigned int flags) {
  (void)target;(void)length;(void)flags;if(mode("map-null"))return NULL;mapped=1;return bytes[buffer]+offset;
}
unsigned char glUnmapBuffer(unsigned int target) {
  (void)target;++unmap_calls;
  if(mode("map-corrupt") && unmap_calls==1)return 0;
  mapped=0;if(mode("copy-error"))error_code=0x0502;return 1;
}
#ifndef OWNED_GL_MISSING_ASYNC
void *glFenceSync(unsigned int condition,unsigned int flags) {
  (void)condition;(void)flags;if(mode("fence-zero"))return NULL;
  uintptr_t value=++next_fence;fences[value-1000]=1;return (void *)value;
}
#endif
unsigned int glClientWaitSync(void *fence,unsigned int flags,uint64_t timeout) {
  (void)fence;(void)flags;(void)timeout;
  if(mode("timeout"))return 0x911B;
  if(mode("wait-error")){error_code=0x0502;return 0x911D;}
  if(mode("unexpected"))return 0xCAFE;
  return 0x911A;
}
void glDeleteSync(void *fence){fences[(uintptr_t)fence-1000]=0;}
void glFlush(void) {}

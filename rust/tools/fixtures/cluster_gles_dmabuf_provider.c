#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>

static int framebuffer=19, texture=23, pack_buffer, alignment=4;
static unsigned int error_code;
static unsigned int next_texture=200,next_framebuffer=300,next_image=500;
static int live_textures,live_framebuffers,live_images,live_fences;
static int imports,destroys,attributes_ok=1,destroy_fd_live=1;
static int import_fds[16];
static int mode(const char *name){const char *value=getenv("OWNED_DMA_SCENARIO");return value && strcmp(value,name)==0;}
static void summary(void){printf("{\"provider_summary\":{\"textures\":%d,\"framebuffers\":%d,\"images\":%d,\"fences\":%d,\"imports\":%d,\"destroys\":%d,\"attributes_ok\":%d,\"destroy_fd_live\":%d,\"bindings\":[%d,%d]}}\n",live_textures,live_framebuffers,live_images,live_fences,imports,destroys,attributes_ok,destroy_fd_live,texture,framebuffer);}
__attribute__((constructor)) static void initialize(void){atexit(summary);}
void *eglGetCurrentDisplay(void){return mode("no-display") ? NULL : (void *)(uintptr_t)0x1234;}
unsigned int eglInitialize(void *display,int *major,int *minor){(void)display;*major=1;*minor=5;return 1;}
const char *eglQueryString(void *display,int name){(void)display;(void)name;return mode("no-extension") ? "" : "EGL_EXT_image_dma_buf_import";}
int eglGetError(void){return 0x300C;}
void *eglCreateImageKHR(void *display,void *context,unsigned int target,void *buffer,const int *attributes){
  (void)display;(void)context;(void)buffer;
  ++imports;
  const int valid=target==0x3270 && attributes[0]==0x3057 && attributes[1]>0 && attributes[2]==0x3056 && attributes[3]>0 && attributes[4]==0x3271 && attributes[5]==0x34324241 && attributes[6]==0x3272 && fcntl(attributes[7],F_GETFD)>=0 && attributes[8]==0x3273 && attributes[9]==0 && attributes[10]==0x3274 && attributes[11]==attributes[1]*4 && attributes[12]==0x3038;
  attributes_ok &= valid;
  if(mode("import-reject"))return NULL;
  unsigned int image=++next_image;import_fds[image-500]=attributes[7];++live_images;return (void *)(uintptr_t)image;
}
unsigned int eglDestroyImageKHR(void *display,void *image){(void)display;destroy_fd_live &= fcntl(import_fds[(uintptr_t)image-500],F_GETFD)>=0;++destroys;--live_images;return 1;}
void glEGLImageTargetTexture2DOES(unsigned int target,void *image){(void)target;(void)image;if(mode("bind-error"))error_code=0x0502;}
void *eglGetProcAddress(const char *name){
  if(mode("missing-functions"))return NULL;
  if(strcmp(name,"eglCreateImageKHR")==0)return (void *)eglCreateImageKHR;
  if(strcmp(name,"eglDestroyImageKHR")==0)return (void *)eglDestroyImageKHR;
  if(strcmp(name,"glEGLImageTargetTexture2DOES")==0)return (void *)glEGLImageTargetTexture2DOES;
  return NULL;
}
void glGetIntegerv(unsigned int name,int *value){switch(name){case 0x8CA6:*value=framebuffer;break;case 0x8069:*value=texture;break;case 0x88ED:*value=pack_buffer;break;case 0x0D05:*value=alignment;break;default:*value=0;}}
unsigned int glGetError(void){if(mode("uncleared"))return 0x0500;unsigned int value=error_code;error_code=0;return value;}
void glBindTexture(unsigned int target,unsigned int name){(void)target;texture=(int)name;}
void glActiveTexture(unsigned int texture_unit){(void)texture_unit;}
void glGenTextures(int count,unsigned int *names){for(int i=0;i<count;++i){names[i]=++next_texture;++live_textures;}}
void glDeleteTextures(int count,const unsigned int *names){(void)names;live_textures-=count;}
void glTexParameteri(unsigned int target,unsigned int name,int value){(void)target;(void)name;(void)value;}
void glBindFramebuffer(unsigned int target,unsigned int name){(void)target;framebuffer=(int)name;}
void glGenFramebuffers(int count,unsigned int *names){for(int i=0;i<count;++i){names[i]=++next_framebuffer;++live_framebuffers;}}
void glDeleteFramebuffers(int count,const unsigned int *names){(void)names;live_framebuffers-=count;}
void glFramebufferTexture2D(unsigned int target,unsigned int attachment,unsigned int texture_target,unsigned int name,int level){(void)target;(void)attachment;(void)texture_target;(void)name;(void)level;if(mode("framebuffer-error"))error_code=0x0502;}
unsigned int glCheckFramebufferStatus(unsigned int target){(void)target;return mode("framebuffer-incomplete") ? 0x8CD6 : 0x8CD5;}
void *glFenceSync(unsigned int condition,unsigned int flags){(void)condition;(void)flags;if(mode("fence-zero"))return NULL;++live_fences;return (void *)(uintptr_t)0x4567;}
unsigned int glClientWaitSync(void *fence,unsigned int flags,uint64_t timeout){
  (void)fence;attributes_ok &= flags==1 && timeout==50000000;
  if(mode("timeout"))return 0x911B;
  if(mode("wait-error")){error_code=0x0502;return 0x911D;}
  return mode("unexpected") ? 0xCAFE : 0x911A;
}
void glDeleteSync(void *fence){(void)fence;--live_fences;}
void glBindBuffer(unsigned int target,unsigned int name){(void)target;pack_buffer=(int)name;}
void glPixelStorei(unsigned int name,int value){if(name==0x0D05)alignment=value;}
void glReadPixels(int x,int y,int width,int height,unsigned int format,unsigned int type,void *destination){(void)x;(void)y;(void)width;(void)height;(void)format;(void)type;(void)destination;}

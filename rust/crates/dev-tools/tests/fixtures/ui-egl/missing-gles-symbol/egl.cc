// A private EGL/GLES ABI fixture. No image is imported from a real device.
#include <cstdlib>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <unistd.h>
#include <fcntl.h>
static int last_error=0x3000;
static int attempts=0;
static int images=0;
static const char *scenario(){const char *value=std::getenv("EGL_CASE");return value?value:"success";}
static FILE *output(){return std::fopen(std::getenv("EGL_TRACE"),"a");}
extern "C" {
void *eglGetCurrentDisplay(){return std::strcmp(scenario(),"no-display")==0?nullptr:reinterpret_cast<void *>(1);}
unsigned int eglInitialize(void *,int *major,int *minor){*major=1;*minor=5;bool fail=std::strcmp(scenario(),"init-fail")==0;last_error=fail?0x3001:0x3000;auto f=output();std::fprintf(f,"[\"initialize\",%s]\n",fail?"false":"true");std::fclose(f);return !fail;}
const char *eglQueryString(void *,int){return "EGL_EXT_image_dma_buf_import";}
int eglGetError(){int error=last_error;last_error=0x3000;return error;}
void *eglCreateImageKHR(void *,void *,unsigned int,void *,const int *attrs){
  ++attempts;bool retry=std::strcmp(scenario(),"retry")==0&&attempts==1;bool failure=std::strcmp(scenario(),"fail")==0||std::strcmp(scenario(),"retry-fail")==0;
  last_error=retry||std::strcmp(scenario(),"retry-fail")==0?0x3001:failure?0x3009:0x3000;
  auto f=output();std::fprintf(f,"[\"create\",%d,%d,%d,%d,%s,%s]\n",attrs[1],attrs[3],attrs[11],attrs[15],fcntl(attrs[7],F_GETFD)>=0?"true":"false",last_error==0x3000?"true":"false");std::fclose(f);
  if(last_error!=0x3000)return nullptr;return new int(++images);
}
unsigned int eglDestroyImageKHR(void *,void *image){delete static_cast<int *>(image);auto f=output();std::fprintf(f,"[\"destroy\"]\n");std::fclose(f);return 1;}
void glActiveTexture(unsigned int texture){auto f=output();std::fprintf(f,"[\"active\",%u]\n",texture);std::fclose(f);}
void glBindTexture(unsigned int target,unsigned int texture){auto f=output();std::fprintf(f,"[\"bind\",%u,%u]\n",target,texture);std::fclose(f);}
void glEGLImageTargetTexture2DOES(unsigned int target,void *image){auto f=output();std::fprintf(f,"[\"image\",%u,%s]\n",target,*static_cast<int *>(image)>0?"true":"false");std::fclose(f);}
void *eglGetProcAddress(const char *name){if(std::strcmp(scenario(),"fallback")==0)return nullptr;return dlsym(RTLD_DEFAULT,name);}
}

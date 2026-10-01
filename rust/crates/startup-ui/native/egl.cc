#include "egl.h"
#include <dlfcn.h>
#include <stdexcept>
#include <string>
namespace startup_ui {
namespace {
struct Library {
  void *handle;
  explicit Library(rust::Str path):handle(dlopen(std::string(path).c_str(),RTLD_NOW|RTLD_LOCAL)) {
    if (!handle) throw std::runtime_error(std::string("EGL library load: ")+dlerror());
  }
  ~Library(){dlclose(handle);}
  template<typename T> T symbol(const char *name) const {
    auto address=dlsym(handle,name);
    if (!address) throw std::runtime_error(std::string("EGL symbol missing: ")+name);
    return reinterpret_cast<T>(address);
  }
};
}
struct EglApi::State {
  Library egl,gles;
  void *(*current)();
  unsigned int (*init)(void *,int *,int *);
  const char *(*query)(void *,int);
  void *(*proc)(const char *);
  int (*get_error)();
  void *(*create)(void *,void *,unsigned int,void *,const int *);
  unsigned int (*destroy)(void *,void *);
  void (*image_target)(unsigned int,void *);
  void (*bind)(unsigned int,unsigned int);
  void (*active)(unsigned int);
  State(rust::Str egl_path,rust::Str gles_path):egl(egl_path),gles(gles_path) {
    current=egl.symbol<decltype(current)>("eglGetCurrentDisplay");
    init=egl.symbol<decltype(init)>("eglInitialize");
    query=egl.symbol<decltype(query)>("eglQueryString");
    proc=egl.symbol<decltype(proc)>("eglGetProcAddress");
    get_error=egl.symbol<decltype(get_error)>("eglGetError");
    bind=gles.symbol<decltype(bind)>("glBindTexture");
    active=gles.symbol<decltype(active)>("glActiveTexture");
    create=extension<decltype(create)>("eglCreateImageKHR",egl);
    destroy=extension<decltype(destroy)>("eglDestroyImageKHR",egl);
    image_target=extension<decltype(image_target)>("glEGLImageTargetTexture2DOES",gles);
  }
  template<typename T>T extension(const char *name,const Library &library) {
    auto address=proc(name);
    return address ? reinterpret_cast<T>(address) : library.symbol<T>(name);
  }
};
EglApi::EglApi(rust::Str egl_path,rust::Str gles_path):state(std::make_unique<State>(egl_path,gles_path)){}
EglApi::~EglApi()=default;
uint64_t EglApi::current_display()const{return reinterpret_cast<uintptr_t>(state->current());}
bool EglApi::initialize(uint64_t display)const{int major=0,minor=0;return state->init(reinterpret_cast<void *>(display),&major,&minor)!=0;}
rust::String EglApi::extensions(uint64_t display)const{auto value=state->query(reinterpret_cast<void *>(display),0x3055);return value ? rust::String(value):rust::String();}
int32_t EglApi::error()const{return state->get_error();}
uint64_t EglApi::create_image(uint64_t display,rust::Slice<const int32_t> attributes)const{
  if (attributes.size()!=19 || attributes[18]!=0x3038)throw std::runtime_error("invalid NV12 EGL attributes");
  return reinterpret_cast<uintptr_t>(state->create(reinterpret_cast<void *>(display),nullptr,0x3270,nullptr,attributes.data()));
}
bool EglApi::destroy_image(uint64_t display,uint64_t image)const{return state->destroy(reinterpret_cast<void *>(display),reinterpret_cast<void *>(image))!=0;}
void EglApi::bind_image(uint32_t texture,uint64_t image)const{state->active(0x84C0);state->bind(0x8D65,texture);state->image_target(0x8D65,reinterpret_cast<void *>(image));}
std::unique_ptr<EglApi> egl_api(rust::Str egl_path,rust::Str gles_path){return std::make_unique<EglApi>(egl_path,gles_path);}
}

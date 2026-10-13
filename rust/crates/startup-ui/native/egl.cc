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
  void *(*create)(void *,void *,unsigned int,void *,const int *)=nullptr;
  unsigned int (*destroy)(void *,void *)=nullptr;
  void (*image_target)(unsigned int,void *)=nullptr;
  void (*bind)(unsigned int,unsigned int);
  void (*active)(unsigned int);
  State(rust::Str egl_path,rust::Str gles_path,bool lazy_images):egl(egl_path),gles(gles_path) {
    current=egl.symbol<decltype(current)>("eglGetCurrentDisplay");
    init=egl.symbol<decltype(init)>("eglInitialize");
    query=egl.symbol<decltype(query)>("eglQueryString");
    proc=egl.symbol<decltype(proc)>("eglGetProcAddress");
    get_error=egl.symbol<decltype(get_error)>("eglGetError");
    bind=gles.symbol<decltype(bind)>("glBindTexture");
    active=gles.symbol<decltype(active)>("glActiveTexture");
    if(!lazy_images) {
      create=extension<decltype(create)>("eglCreateImageKHR",egl);
      destroy=extension<decltype(destroy)>("eglDestroyImageKHR",egl);
      image_target=extension<decltype(image_target)>("glEGLImageTargetTexture2DOES",gles);
    }
  }
  template<typename T>T extension(const char *name,const Library &library) {
    auto address=proc(name);
    return address ? reinterpret_cast<T>(address) : library.symbol<T>(name);
  }
};
EglApi::EglApi(rust::Str egl_path,rust::Str gles_path,bool lazy_images):state(std::make_unique<State>(egl_path,gles_path,lazy_images)){}
EglApi::~EglApi()=default;
uint64_t EglApi::current_display()const{return reinterpret_cast<uintptr_t>(state->current());}
bool EglApi::initialize(uint64_t display)const{int major=0,minor=0;return state->init(reinterpret_cast<void *>(display),&major,&minor)!=0;}
rust::String EglApi::extensions(uint64_t display)const{auto value=state->query(reinterpret_cast<void *>(display),0x3055);return value ? rust::String(value):rust::String();}
int32_t EglApi::error()const{return state->get_error();}
uint64_t EglApi::create_image(uint64_t display,rust::Slice<const int32_t> attributes)const{
  if (attributes.empty() || attributes.size()%2!=1 || attributes[attributes.size()-1]!=0x3038)throw std::runtime_error("invalid terminated EGL attributes");
  if(!state->create || !state->destroy || !state->image_target)throw std::runtime_error("EGLImage functions unavailable");
  return reinterpret_cast<uintptr_t>(state->create(reinterpret_cast<void *>(display),nullptr,0x3270,nullptr,attributes.data()));
}
bool EglApi::destroy_image(uint64_t display,uint64_t image)const{return state->destroy(reinterpret_cast<void *>(display),reinterpret_cast<void *>(image))!=0;}
void EglApi::bind_image(uint32_t texture,uint64_t image)const{state->active(0x84C0);state->bind(0x8D65,texture);state->image_target(0x8D65,reinterpret_cast<void *>(image));}
bool EglApi::prepare_images()const {
  auto create=state->proc("eglCreateImageKHR"),destroy=state->proc("eglDestroyImageKHR"),target=state->proc("glEGLImageTargetTexture2DOES");
  if(!create || !destroy || !target)return false;
  state->create=reinterpret_cast<decltype(state->create)>(create);
  state->destroy=reinterpret_cast<decltype(state->destroy)>(destroy);
  state->image_target=reinterpret_cast<decltype(state->image_target)>(target);
  return true;
}
void EglApi::image_target(uint32_t target,uint64_t image)const {
  if(!state->image_target)throw std::runtime_error("EGL image target function unavailable");
  state->image_target(target,reinterpret_cast<void *>(image));
}
std::unique_ptr<EglApi> egl_api(rust::Str egl_path,rust::Str gles_path){return std::make_unique<EglApi>(egl_path,gles_path);}
std::unique_ptr<EglApi> egl_image_api(rust::Str egl_path,rust::Str gles_path){return std::make_unique<EglApi>(egl_path,gles_path,true);}
}

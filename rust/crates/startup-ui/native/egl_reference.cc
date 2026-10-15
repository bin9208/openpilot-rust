#include "egl_reference.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include <dlfcn.h>
#include <cerrno>
#include <unistd.h>
#include <map>
#include <limits>
#include <stdexcept>
#include <string>
namespace startup_ui {
  namespace {
    struct Library {
      void* handle=nullptr;
      ~Library() {
        if (handle)dlclose(handle);
      }
      void load(const char* name) {
        if (handle)throw std::runtime_error("reference library already loaded");
        handle=dlopen(name,RTLD_NOW|RTLD_LOCAL);
        if (!handle)throw std::runtime_error(dlerror());
      }
      void* symbol(const char* name)const {
        if (!handle)throw std::runtime_error("reference library not loaded");
        auto address=dlsym(handle,name);
        if (!address)throw std::runtime_error(std::string("reference symbol missing: ")+name);
        return address;
      }
    };
    template<class T>T required(T function) {
      if (!function)throw std::runtime_error("reference function not resolved");
      return function;
    }
    const char* name(ReferenceExtension symbol) {
      switch (symbol) {
        case ReferenceExtension::Create:
          return "eglCreateImageKHR";
        case ReferenceExtension::Destroy:
          return "eglDestroyImageKHR";
        case ReferenceExtension::ImageTarget:
          return "glEGLImageTargetTexture2DOES";
        default:
          throw std::runtime_error("invalid reference extension");
      }
    }
  }
  struct ReferenceApi::State {
    Library egl,gles;
    void*(*current)()=nullptr;
    unsigned int(*initialize)(void*,int*,int*)=nullptr;
    const char*(*query)(void*,int)=nullptr;
    void*(*proc)(const char*)=nullptr;
    int(*error)()=nullptr;
    void*(*create)(void*,void*,unsigned int,void*,const int*)=nullptr;
    unsigned int(*destroy)(void*,void*)=nullptr;
    void(*image)(unsigned int,void*)=nullptr;
    void(*active)(unsigned int)=nullptr;
    void(*bind)(unsigned int,unsigned int)=nullptr;
    std::map<std::uint64_t,void*> displays;
    struct Image {
      std::uint64_t display;
      void* address;
    };
    std::map<std::uint64_t,Image> images;
    std::uint64_t next=1;
    std::uint64_t token() {
      if (next==std::numeric_limits<std::uint64_t>::max())throw std::runtime_error("reference handle overflow");
      return next++;
    }
    void* display(std::uint64_t token)const {
      auto found=displays.find(token);
      if (found==displays.end())throw std::runtime_error("unknown reference display");
      return found->second;
    }
    const Image& get_image(std::uint64_t token)const {
      auto found=images.find(token);
      if (found==images.end())throw std::runtime_error("unknown or destroyed reference image");
      return found->second;
    }
    void assign(ReferenceExtension slot,void* address) {
      switch (slot) {
        case ReferenceExtension::Create:
          create=reinterpret_cast<decltype(create)>(address);
          break;
        case ReferenceExtension::Destroy:
          destroy=reinterpret_cast<decltype(destroy)>(address);
          break;
        case ReferenceExtension::ImageTarget:
          image=reinterpret_cast<decltype(image)>(address);
          break;
        default:
          throw std::runtime_error("invalid reference extension");
      }
    }
  };
  ReferenceApi::ReferenceApi():state(std::make_unique<State>()) {
  }
  ReferenceApi::~ReferenceApi()=default;
  void ReferenceApi::load_egl() {
    state->egl.load("libEGL.so");
  }
  void ReferenceApi::load_gles() {
    state->gles.load("libGLESv2.so");
  }
  void ReferenceApi::resolve_core(ReferenceCore slot) {
    switch (slot) {
      case ReferenceCore::Current:
        state->current=reinterpret_cast<decltype(state->current)>(state->egl.symbol("eglGetCurrentDisplay"));
        break;
      case ReferenceCore::Initialize:
        state->initialize=reinterpret_cast<decltype(state->initialize)>(state->egl.symbol("eglInitialize"));
        break;
      case ReferenceCore::Query:
        state->query=reinterpret_cast<decltype(state->query)>(state->egl.symbol("eglQueryString"));
        break;
      case ReferenceCore::Proc:
        state->proc=reinterpret_cast<decltype(state->proc)>(state->egl.symbol("eglGetProcAddress"));
        break;
      case ReferenceCore::Error:
        state->error=reinterpret_cast<decltype(state->error)>(state->egl.symbol("eglGetError"));
        break;
      case ReferenceCore::Bind:
        state->bind=reinterpret_cast<decltype(state->bind)>(state->gles.symbol("glBindTexture"));
        break;
      case ReferenceCore::Active:
        state->active=reinterpret_cast<decltype(state->active)>(state->gles.symbol("glActiveTexture"));
        break;
      default:
        throw std::runtime_error("invalid reference core symbol");
    }
  }
  bool ReferenceApi::resolve_proc(ReferenceExtension slot) {
    auto address=required(state->proc)(name(slot));
    if (!address)return false;
    state->assign(slot,address);
    return true;
  }
  void ReferenceApi::resolve_library(ReferenceExtension slot) {
    auto& library=slot==ReferenceExtension::ImageTarget?state->gles:state->egl;
    state->assign(slot,library.symbol(name(slot)));
  }
  std::uint64_t ReferenceApi::reference_display() {
    auto address=required(state->current)();
    if (!address)return 0;
    for (const auto& entry:state->displays)if (entry.second==address)return entry.first;
    auto token=state->token();
    state->displays.emplace(token,address);
    return token;
  }
  bool ReferenceApi::reference_initialize(std::uint64_t display)const {
    auto address=state->display(display);
    int major=0,minor=0;
    return required(state->initialize)(address,&major,&minor)!=0;
  }
  rust::Vec<std::uint8_t> ReferenceApi::reference_extensions(std::uint64_t display)const {
    auto address=state->display(display);
    auto value=required(state->query)(address,0x3055);
    rust::Vec<std::uint8_t> result;
    if (value)while (*value)result.push_back(static_cast<std::uint8_t>(*value++));
    return result;
  }
  std::int32_t ReferenceApi::reference_error()const {
    return required(state->error)();
  }
  std::uint64_t ReferenceApi::reference_create(std::uint64_t display,std::uint32_t target,rust::Slice<const std::int32_t> attributes) {
    auto address=state->display(display);
    if (attributes.size()!=19||attributes[18]!=0x3038)throw std::runtime_error("invalid reference EGL attribute array");
    auto image=required(state->create)(address,nullptr,target,nullptr,attributes.data());
    if (!image)return 0;
    auto token=state->token();
    state->images.emplace(token,State::Image {
      display,image
    });
    return token;
  }
  void ReferenceApi::reference_active(std::uint32_t texture)const {
    required(state->active)(texture);
  }
  void ReferenceApi::reference_bind(std::uint32_t target,std::uint32_t texture)const {
    required(state->bind)(target,texture);
  }
  void ReferenceApi::reference_image_target(std::uint32_t target,std::uint64_t image)const {
    const auto& handle=state->get_image(image);
    required(state->image)(target,handle.address);
  }
  bool ReferenceApi::reference_destroy(std::uint64_t display,std::uint64_t image) {
    auto address=state->display(display);
    const auto handle=state->get_image(image);
    if (handle.display!=display)throw std::runtime_error("reference image display mismatch");
    auto result=required(state->destroy)(address,handle.address)!=0;
    state->images.erase(image);
    return result;
  }
  std::unique_ptr<ReferenceApi> reference_api() {
    return std::make_unique<ReferenceApi>();
  }
  // Called only by the safe Rust wrapper after consuming an OwnedFd. A foreign
  // EGL implementation may already have closed it; report errno without abort.
  std::int32_t reference_close_owned_descriptor(std::int32_t fd) {
    if (::close(fd) == 0) return 0;
    return errno;
  }

}

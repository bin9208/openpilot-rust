#include "gles.h"
#include <dlfcn.h>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string>
namespace startup_ui {
namespace {
template<typename T> T require(T function,const char *name) {
  if(!function)throw std::runtime_error(std::string("GLES symbol missing: ")+name);
  return function;
}
int count(size_t value) {
  if(value>size_t(std::numeric_limits<int>::max()))throw std::runtime_error("GLES element count exceeds native range");
  return int(value);
}
}
struct GlesApi::State {
  void *library;
  void (*get_integer)(unsigned int,int *);
  unsigned int (*get_error)();
  void (*bind_framebuffer)(unsigned int,unsigned int);
  void (*bind_buffer)(unsigned int,unsigned int);
  void (*pixel_store)(unsigned int,int);
  void (*read_pixels)(int,int,int,int,unsigned int,unsigned int,void *);
  void (*gen_buffers)(int,unsigned int *);
  void (*delete_buffers)(int,const unsigned int *);
  void (*buffer_data)(unsigned int,ptrdiff_t,const void *,unsigned int);
  void (*buffer_parameter)(unsigned int,unsigned int,int *);
  void *(*map_range)(unsigned int,ptrdiff_t,ptrdiff_t,unsigned int);
  unsigned char (*unmap)(unsigned int);
  void *(*fence)(unsigned int,unsigned int);
  unsigned int (*wait_fence)(void *,unsigned int,uint64_t);
  void (*delete_fence)(void *);
  void (*flush)();
  void (*gen_textures)(int,unsigned int *);
  void (*delete_textures)(int,const unsigned int *);
  void (*bind_texture)(unsigned int,unsigned int);
  void (*texture_parameter)(unsigned int,unsigned int,int);
  void (*gen_framebuffers)(int,unsigned int *);
  void (*delete_framebuffers)(int,const unsigned int *);
  void (*attach_texture)(unsigned int,unsigned int,unsigned int,unsigned int,int);
  unsigned int (*framebuffer_status)(unsigned int);
  explicit State(rust::Str path):library(dlopen(std::string(path).c_str(),RTLD_NOW|RTLD_LOCAL)) {
    if(!library)throw std::runtime_error(std::string("GLES library load: ")+dlerror());
#define LOAD(field,name) field=reinterpret_cast<decltype(field)>(dlsym(library,name))
    LOAD(get_integer,"glGetIntegerv");LOAD(get_error,"glGetError");
    LOAD(bind_framebuffer,"glBindFramebuffer");LOAD(bind_buffer,"glBindBuffer");
    LOAD(pixel_store,"glPixelStorei");LOAD(read_pixels,"glReadPixels");
    LOAD(gen_buffers,"glGenBuffers");LOAD(delete_buffers,"glDeleteBuffers");
    LOAD(buffer_data,"glBufferData");LOAD(buffer_parameter,"glGetBufferParameteriv");
    LOAD(map_range,"glMapBufferRange");LOAD(unmap,"glUnmapBuffer");
    LOAD(fence,"glFenceSync");LOAD(wait_fence,"glClientWaitSync");
    LOAD(delete_fence,"glDeleteSync");LOAD(flush,"glFlush");
    LOAD(gen_textures,"glGenTextures");LOAD(delete_textures,"glDeleteTextures");
    LOAD(bind_texture,"glBindTexture");LOAD(texture_parameter,"glTexParameteri");
    LOAD(gen_framebuffers,"glGenFramebuffers");LOAD(delete_framebuffers,"glDeleteFramebuffers");
    LOAD(attach_texture,"glFramebufferTexture2D");LOAD(framebuffer_status,"glCheckFramebufferStatus");
#undef LOAD
    try {
      require(get_integer,"glGetIntegerv");require(get_error,"glGetError");
      require(bind_framebuffer,"glBindFramebuffer");require(bind_buffer,"glBindBuffer");
      require(pixel_store,"glPixelStorei");require(read_pixels,"glReadPixels");
    } catch(...) {dlclose(library);throw;}
  }
  ~State(){dlclose(library);}
};
GlesApi::GlesApi(rust::Str path):state(std::make_shared<State>(path)){}
GlesApi::~GlesApi()=default;
std::unique_ptr<GlesApi> gles_api(rust::Str path){return std::make_unique<GlesApi>(path);}
int32_t GlesApi::integer(uint32_t parameter)const{int result=0;state->get_integer(parameter,&result);return result;}
uint32_t GlesApi::gl_error()const{return state->get_error();}
void GlesApi::bind_framebuffer(uint32_t target,uint32_t name)const{state->bind_framebuffer(target,name);}
void GlesApi::bind_buffer(uint32_t target,uint32_t name)const{state->bind_buffer(target,name);}
void GlesApi::pixel_store(uint32_t parameter,int32_t value)const{state->pixel_store(parameter,value);}
uint64_t GlesApi::read_extent(int32_t width,int32_t height)const {
  if(width<=0 || height<=0)throw std::runtime_error("invalid GLES RGBA dimensions");
  const int alignment=integer(0x0D05),row_length=integer(0x0D02),skip_rows=integer(0x0D03),skip_pixels=integer(0x0D04);
  if((alignment!=1 && alignment!=2 && alignment!=4 && alignment!=8) || row_length<0 || skip_rows<0 || skip_pixels<0)throw std::runtime_error("invalid GLES pixel-pack state");
  const uint64_t row=uint64_t(row_length ? row_length : width)*4;
  const uint64_t stride=(row+uint64_t(alignment)-1)/uint64_t(alignment)*uint64_t(alignment);
  if(uint64_t(skip_rows)+uint64_t(height)-1>std::numeric_limits<uint64_t>::max()/stride)throw std::runtime_error("GLES pixel-pack extent overflow");
  const uint64_t prefix=(uint64_t(skip_rows)+uint64_t(height)-1)*stride;
  const uint64_t tail=(uint64_t(skip_pixels)+uint64_t(width))*4;
  if(tail>std::numeric_limits<uint64_t>::max()-prefix)throw std::runtime_error("GLES pixel-pack extent overflow");
  return prefix+tail;
}
void GlesApi::read_rgba(int32_t width,int32_t height,rust::Slice<uint8_t> destination)const {
  if(integer(0x88ED)!=0)throw std::runtime_error("direct GLES RGBA destination requires no pack buffer");
  if(read_extent(width,height)>destination.size())throw std::runtime_error("GLES RGBA destination is too short");
  state->read_pixels(0,0,width,height,0x1908,0x1401,destination.data());
}
void GlesApi::read_rgba_pbo(int32_t width,int32_t height)const {
  if(integer(0x88ED)==0)throw std::runtime_error("GLES asynchronous read requires a pack buffer");
  int size=0;require(state->buffer_parameter,"glGetBufferParameteriv")(0x88EB,0x8764,&size);
  if(size<0 || read_extent(width,height)>uint64_t(size))throw std::runtime_error("GLES pack buffer is too short");
  state->read_pixels(0,0,width,height,0x1908,0x1401,nullptr);
}
bool GlesApi::async_available()const{return state->gen_buffers && state->delete_buffers && state->buffer_data && state->buffer_parameter && state->map_range && state->unmap && state->fence && state->wait_fence && state->delete_fence && state->flush;}
rust::Vec<uint32_t> GlesApi::generate_buffers(uint32_t amount)const {
  const int native_count=count(amount);
  rust::Vec<uint32_t> values;values.reserve(amount);for(uint32_t i=0;i<amount;++i)values.push_back(0);
  require(state->gen_buffers,"glGenBuffers")(native_count,values.data());return values;
}
void GlesApi::prepare_dma()const {
  require(state->gen_textures,"glGenTextures");require(state->delete_textures,"glDeleteTextures");
  require(state->bind_texture,"glBindTexture");require(state->texture_parameter,"glTexParameteri");
  require(state->gen_framebuffers,"glGenFramebuffers");require(state->delete_framebuffers,"glDeleteFramebuffers");
  require(state->attach_texture,"glFramebufferTexture2D");require(state->framebuffer_status,"glCheckFramebufferStatus");
  require(state->fence,"glFenceSync");require(state->wait_fence,"glClientWaitSync");require(state->delete_fence,"glDeleteSync");
}
void GlesApi::delete_buffers(rust::Slice<const uint32_t> values)const{require(state->delete_buffers,"glDeleteBuffers")(count(values.size()),values.data());}
void GlesApi::allocate_buffer(uint32_t target,uint64_t size,uint32_t usage)const {
  if(size>uint64_t(std::numeric_limits<ptrdiff_t>::max()))throw std::runtime_error("GLES buffer size exceeds native range");
  require(state->buffer_data,"glBufferData")(target,ptrdiff_t(size),nullptr,usage);
}
uint64_t GlesApi::fence(uint32_t condition,uint32_t flags)const{return reinterpret_cast<uintptr_t>(require(state->fence,"glFenceSync")(condition,flags));}
uint32_t GlesApi::wait_fence(uint64_t fence,uint32_t flags,uint64_t timeout)const{return require(state->wait_fence,"glClientWaitSync")(reinterpret_cast<void *>(uintptr_t(fence)),flags,timeout);}
void GlesApi::delete_fence(uint64_t fence)const{require(state->delete_fence,"glDeleteSync")(reinterpret_cast<void *>(uintptr_t(fence)));}
void GlesApi::flush()const{require(state->flush,"glFlush")();}
std::unique_ptr<GlMapping> GlesApi::map_read(uint32_t target,uint64_t size)const{return std::make_unique<GlMapping>(state,target,size);}
GlMapping::GlMapping(std::shared_ptr<GlesApi::State> api,uint32_t target,uint64_t size):api(std::move(api)),target(target),buffer(0),size(size),address(nullptr) {
  if(target!=0x88EB || size==0 || size>uint64_t(std::numeric_limits<ptrdiff_t>::max()))throw std::runtime_error("invalid GLES read mapping");
  int bound=0,capacity=0;this->api->get_integer(0x88ED,&bound);buffer=uint32_t(bound);
  require(this->api->buffer_parameter,"glGetBufferParameteriv")(target,0x8764,&capacity);
  if(buffer==0 || capacity<0 || size>uint64_t(capacity))throw std::runtime_error("GLES read mapping exceeds buffer");
  require(this->api->unmap,"glUnmapBuffer");
  address=require(this->api->map_range,"glMapBufferRange")(target,0,ptrdiff_t(size),0x0001);
  if(!address)throw std::runtime_error("GLES read mapping failed");
}
GlMapping::~GlMapping(){if(address)unmap();}
void GlMapping::copy_to(rust::Slice<uint8_t> destination)const {
  if(!address || size>destination.size())throw std::runtime_error("GLES mapped destination is too short");
  std::memcpy(destination.data(),address,size_t(size));
}
bool GlMapping::unmap(){
  if(!address)return true;
  int previous=0;api->get_integer(0x88ED,&previous);api->bind_buffer(target,buffer);
  const bool result=require(api->unmap,"glUnmapBuffer")(target)!=0;
  api->bind_buffer(target,uint32_t(previous));if(result)address=nullptr;return result;
}
uint32_t GlesApi::generate_texture()const{unsigned int value=0;require(state->gen_textures,"glGenTextures")(1,&value);return value;}
void GlesApi::delete_texture(uint32_t value)const{require(state->delete_textures,"glDeleteTextures")(1,&value);}
void GlesApi::bind_texture(uint32_t target,uint32_t value)const{require(state->bind_texture,"glBindTexture")(target,value);}
void GlesApi::texture_parameter(uint32_t target,uint32_t parameter,int32_t value)const{require(state->texture_parameter,"glTexParameteri")(target,parameter,value);}
uint32_t GlesApi::generate_framebuffer()const{unsigned int value=0;require(state->gen_framebuffers,"glGenFramebuffers")(1,&value);return value;}
void GlesApi::delete_framebuffer(uint32_t value)const{require(state->delete_framebuffers,"glDeleteFramebuffers")(1,&value);}
void GlesApi::attach_texture(uint32_t target,uint32_t attachment,uint32_t texture)const{require(state->attach_texture,"glFramebufferTexture2D")(target,attachment,0x0DE1,texture,0);}
uint32_t GlesApi::framebuffer_status(uint32_t target)const{return require(state->framebuffer_status,"glCheckFramebufferStatus")(target);}
}

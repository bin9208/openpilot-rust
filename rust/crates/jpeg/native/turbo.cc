#include "turbo.h"
#include "openpilot-jpeg/src/bridge.rs.h"
#include "vendor/src/turbojpeg.h"
#include <dlfcn.h>
#include <stdexcept>
#include <string>
namespace openpilot_jpeg {
struct Turbo::Impl {
  void* library=nullptr;
  decltype(&tjInitCompress) initialize=nullptr;
  decltype(&tjCompress2) compress=nullptr;
  decltype(&tjFree) release=nullptr;
  decltype(&tjDestroy) destroy=nullptr;
  decltype(&tjGetErrorStr) error=nullptr;
  ~Impl() { if(library) dlclose(library); }
  template<class T> T symbol(const char* name) {
    void* address=dlsym(library,name);
    if(!address) throw std::runtime_error(std::string("TurboJPEG missing symbol: ")+name);
    return reinterpret_cast<T>(address);
  }
};
Turbo::Turbo(rust::Slice<const std::uint8_t> bytes):impl(std::make_unique<Impl>()) {
  std::string path(reinterpret_cast<const char*>(bytes.data()),bytes.size());
  if(path.find('\0')!=std::string::npos) throw std::invalid_argument("TurboJPEG library path contains NUL");
  impl->library=dlopen(path.c_str(),RTLD_NOW|RTLD_LOCAL);
  if(!impl->library) { const char* error=dlerror(); throw std::runtime_error(error ? error : "TurboJPEG library unavailable"); }
  impl->initialize=impl->symbol<decltype(impl->initialize)>("tjInitCompress");
  impl->compress=impl->symbol<decltype(impl->compress)>("tjCompress2");
  impl->release=impl->symbol<decltype(impl->release)>("tjFree");
  impl->destroy=impl->symbol<decltype(impl->destroy)>("tjDestroy");
  impl->error=impl->symbol<decltype(impl->error)>("tjGetErrorStr");
}
Turbo::~Turbo()=default;
rust::Vec<std::uint8_t> Turbo::encode(rust::Slice<const std::uint8_t> pixels,Layout layout,std::uint8_t quality) {
  if(!layout.width || !layout.height || layout.width>65500 || layout.height>65500 || layout.components!=4)
    throw std::invalid_argument("TurboJPEG requires packed RGBA dimensions 1..65500");
  if(!quality || quality>100) throw std::invalid_argument("TurboJPEG quality must be 1..100");
  const std::uint64_t expected=std::uint64_t{layout.width}*layout.height*4;
  if(expected!=pixels.size()) throw std::invalid_argument("TurboJPEG packed RGBA length mismatch");
  const std::unique_ptr<void,decltype(impl->destroy)> compressor(impl->initialize(),impl->destroy);
  if(!compressor) throw std::runtime_error(impl->error());
  unsigned char* output=nullptr; unsigned long size=0;
  const int result=impl->compress(compressor.get(),pixels.data(),static_cast<int>(layout.width),0,static_cast<int>(layout.height),TJPF_RGBA,&output,&size,TJSAMP_420,quality,0);
  const std::unique_ptr<unsigned char,decltype(impl->release)> owned(output,impl->release);
  if(result!=0) throw std::runtime_error(impl->error());
  if(!output && size) throw std::runtime_error("TurboJPEG returned invalid output");
  rust::Vec<std::uint8_t> bytes;bytes.reserve(size);
  for(unsigned long i=0;i<size;++i) bytes.push_back(output[i]);
  return bytes;
}
std::unique_ptr<Turbo> open_turbo(rust::Slice<const std::uint8_t> path) {return std::make_unique<Turbo>(path);}
rust::Vec<std::uint8_t> encode_turbo(Turbo& codec,rust::Slice<const std::uint8_t> pixels,Layout layout,std::uint8_t quality) {return codec.encode(pixels,layout,quality);}
}

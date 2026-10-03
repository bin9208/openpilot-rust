#include "bridge.h"
#include "encode.h"
#include <cstdlib>
#include <memory>
#include <stdexcept>
namespace openpilot_jpeg {
rust::Vec<std::uint8_t> encode(rust::Slice<const std::uint8_t> rgb,std::uint32_t width,std::uint32_t height) {
  if (!width || !height || width>65500 || height>65500) throw std::invalid_argument("JPEG dimensions do not match RGB slice");
  const std::uint64_t length=std::uint64_t{width}*height*3;
  if (length!=rgb.size()) throw std::invalid_argument("JPEG dimensions do not match RGB slice");
  unsigned char *output=nullptr;
  unsigned long size=0;
  char error[256]={0};
  if (!encode_rgb(rgb.data(),width,height,&output,&size,error,sizeof(error))) throw std::runtime_error(error);
  const std::unique_ptr<unsigned char,decltype(&std::free)> owned(output,&std::free);
  rust::Vec<std::uint8_t> result;
  result.reserve(size);
  for (unsigned long i=0;i<size;++i) result.push_back(output[i]);
  return result;
}
}

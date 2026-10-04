#include "bridge.h"
#include "encode.h"
#include "openpilot-jpeg/src/bridge.rs.h"
#include <cstdlib>
#include <memory>
#include <stdexcept>
namespace openpilot_jpeg {
rust::Vec<std::uint8_t> encode(rust::Slice<const std::uint8_t> rgb,std::uint32_t width,std::uint32_t height) {
  if (!width || !height || width>65500 || height>65500) throw std::invalid_argument("JPEG dimensions do not match RGB slice");
  const std::uint64_t length=std::uint64_t{width}*height*3;
  if (length!=rgb.size()) throw std::invalid_argument("JPEG dimensions do not match RGB slice");
  return encode_with(rgb,Layout{width,height,3},75);
}
rust::Vec<std::uint8_t> encode_with(rust::Slice<const std::uint8_t> pixels,Layout layout,std::uint8_t quality) {
  if (!layout.width || !layout.height || layout.width>65500 || layout.height>65500 || (layout.components!=1 && layout.components!=3))
    throw std::invalid_argument("JPEG dimensions/components do not match packed pixels");
  if (!quality || quality>100) throw std::invalid_argument("JPEG quality must be 1..100");
  const std::uint64_t length=std::uint64_t{layout.width}*layout.height*layout.components;
  if (length!=pixels.size()) throw std::invalid_argument("JPEG dimensions/components do not match packed pixels");
  unsigned char *output=nullptr;
  unsigned long size=0;
  char error[256]={0};
  if (!encode_pixels(pixels.data(),layout.width,layout.height,layout.components,quality,&output,&size,error,sizeof(error))) throw std::runtime_error(error);
  const std::unique_ptr<unsigned char,decltype(&std::free)> owned(output,&std::free);
  rust::Vec<std::uint8_t> result;
  result.reserve(size);
  for (unsigned long i=0;i<size;++i) result.push_back(output[i]);
  return result;
}
}

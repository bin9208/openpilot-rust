#pragma once
#include <cstdint>
#include "rust/cxx.h"
namespace openpilot_jpeg {
struct Layout;
struct Decoded;
Decoded decode(rust::Slice<const std::uint8_t> bytes,std::uint64_t max_pixels);
rust::Vec<std::uint8_t> encode(rust::Slice<const std::uint8_t> rgb,std::uint32_t width,std::uint32_t height);
rust::Vec<std::uint8_t> encode_with(rust::Slice<const std::uint8_t> pixels,Layout layout,std::uint8_t quality);
}

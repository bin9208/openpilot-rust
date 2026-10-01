#pragma once
#include <cstdint>
#include "rust/cxx.h"
namespace openpilot_jpeg {
rust::Vec<std::uint8_t> encode(rust::Slice<const std::uint8_t> rgb,std::uint32_t width,std::uint32_t height);
}

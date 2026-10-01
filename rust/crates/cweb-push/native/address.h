#pragma once
#include "rust/cxx.h"
namespace openpilot::cweb {
rust::String interface_ipv4(rust::Slice<const std::uint8_t> name);
}

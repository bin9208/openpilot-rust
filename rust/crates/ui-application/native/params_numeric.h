#pragma once
#include "rust/cxx.h"
namespace product_ui {
struct FloatResult;
FloatResult params_float(rust::Slice<const uint8_t> bytes) noexcept;
}

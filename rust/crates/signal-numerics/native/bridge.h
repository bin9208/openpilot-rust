#pragma once
#include "rust/cxx.h"
namespace openpilot_signal_numeric {
void sort_v3(rust::Slice<const float>, rust::Slice<std::size_t>);
void sort_v4(rust::Slice<const float>, rust::Slice<std::size_t>);
}

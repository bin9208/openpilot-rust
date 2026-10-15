#include "bridge.h"
#include "x86simdsort-static-incl.h"
namespace openpilot_signal_numeric {
void sort_v3(rust::Slice<const float> values, rust::Slice<std::size_t> indices) {
  if (values.size() != indices.size()) std::abort();
  x86simdsortStatic::argsort(values.data(), indices.data(), values.size(), true);
}
}

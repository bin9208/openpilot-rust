#pragma once
#include <memory>
#include "rust/cxx.h"
namespace openpilot_pocketfft {
struct Complex;
class Plan {
 public:
  explicit Plan(std::size_t size);
  ~Plan();
  void transform(rust::Slice<Complex> data, double scale, bool forward);
 private:
  struct Impl;
  std::unique_ptr<Impl> impl;
};
std::unique_ptr<Plan> plan(std::size_t size);
}

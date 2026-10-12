#pragma once
#include "rust/cxx.h"
#include <memory>
namespace openpilot_jpeg {
struct Layout;
class Turbo final {
 public:
  explicit Turbo(rust::Slice<const std::uint8_t> path);
  ~Turbo();
  Turbo(const Turbo&)=delete;
  Turbo& operator=(const Turbo&)=delete;
  rust::Vec<std::uint8_t> encode(rust::Slice<const std::uint8_t> pixels,Layout layout,std::uint8_t quality);
 private:
  struct Impl;
  std::unique_ptr<Impl> impl;
};
std::unique_ptr<Turbo> open_turbo(rust::Slice<const std::uint8_t> path);
rust::Vec<std::uint8_t> encode_turbo(Turbo& codec,rust::Slice<const std::uint8_t> pixels,Layout layout,std::uint8_t quality);
}

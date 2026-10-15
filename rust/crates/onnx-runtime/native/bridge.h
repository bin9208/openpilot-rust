#pragma once
#include <memory>
#include "rust/cxx.h"
namespace openpilot_ort {
struct Tensor;
class Session {
public:
  struct Impl;
  explicit Session(std::unique_ptr<Impl>);
  ~Session();
  std::unique_ptr<Impl> value;
};
std::unique_ptr<Session> load(rust::Str, rust::Str);
rust::Vec<Tensor> run(Session&, rust::Str, rust::Slice<const float>, rust::Slice<const std::int64_t>);
}

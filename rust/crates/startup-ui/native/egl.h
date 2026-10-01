#pragma once
#include "rust/cxx.h"
#include <memory>
namespace startup_ui {
class EglApi {
public:
  EglApi(rust::Str egl_path, rust::Str gles_path);
  ~EglApi();
  uint64_t current_display() const;
  bool initialize(uint64_t display) const;
  rust::String extensions(uint64_t display) const;
  int32_t error() const;
  uint64_t create_image(uint64_t display, rust::Slice<const int32_t> attributes) const;
  bool destroy_image(uint64_t display,uint64_t image) const;
  void bind_image(uint32_t texture,uint64_t image) const;
private:
  struct State;
  std::unique_ptr<State> state;
};
std::unique_ptr<EglApi> egl_api(rust::Str egl_path,rust::Str gles_path);
}

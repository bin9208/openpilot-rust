#pragma once
#include "rust/cxx.h"
#include <cstdint>
#include <memory>
namespace startup_ui {
// Raw checked raylib calls only. Scene, shader and frame policy lives in Rust.
class PolygonReferenceApi {
public:
  explicit PolygonReferenceApi(rust::Slice<const std::uint8_t> library);
  ~PolygonReferenceApi();
  void polygon_flags(std::uint32_t flags);
  void polygon_initialize(std::int32_t width, std::int32_t height,
                          rust::Str title);
  void polygon_fps(std::int32_t fps);
  void polygon_begin();
  void polygon_clear(std::uint32_t color);
  void polygon_load_shader(rust::Str vertex, rust::Str fragment);
  void polygon_location(rust::Str name);
  void polygon_integer(rust::Str name, std::int32_t value);
  void polygon_floats(rust::Str name, rust::Slice<const float> values,
                      std::uint32_t components, bool vector);
  void polygon_matrix(rust::Str name, rust::Slice<const float> values);
  void polygon_begin_shader();
  void polygon_strip(rust::Slice<const float> points, std::uint32_t color);
  void polygon_end_shader();
  void polygon_flush();
  std::uint64_t polygon_load_capture();
  bool polygon_export_capture(std::uint64_t image,
                              rust::Slice<const std::uint8_t> path);
  void polygon_unload_capture(std::uint64_t image);
  void polygon_end();
  void polygon_unload_shader();
  void polygon_close();

private:
  struct State;
  std::unique_ptr<State> state;
};
std::unique_ptr<PolygonReferenceApi>
polygon_reference_api(rust::Slice<const std::uint8_t> library);
} // namespace startup_ui

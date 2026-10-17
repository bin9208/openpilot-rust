#include "polygon_reference.h"
#include "raylib.h"
#include <dlfcn.h>
#include <limits>
#include <map>
#include <stdexcept>
#include <string>
#include <vector>
namespace startup_ui {
namespace {
std::string bytes(rust::Slice<const std::uint8_t> input) {
  std::string value(reinterpret_cast<const char *>(input.data()), input.size());
  if (value.find('\0') != std::string::npos)
    throw std::runtime_error("NUL in reference ABI path");
  return value;
}
std::string text(rust::Str input) {
  std::string value(input);
  if (value.find('\0') != std::string::npos)
    throw std::runtime_error("NUL in reference shader text");
  return value;
}
Color color(std::uint32_t value) {
  return Color{std::uint8_t(value), std::uint8_t(value >> 8),
               std::uint8_t(value >> 16), std::uint8_t(value >> 24)};
}
} // namespace
struct PolygonReferenceApi::State {
  void *library = nullptr;
  bool window = false, frame = false, mode = false, loaded = false;
  Shader shader{};
  std::map<std::string, int> locations;
  std::map<std::uint64_t, Image> images;
  std::uint64_t next_image = 1;
  template <typename T> T symbol(const char *name) const {
    auto *value = dlsym(library, name);
    if (!value)
      throw std::runtime_error(std::string("polygon reference ABI missing ") +
                               name);
    return reinterpret_cast<T>(value);
  }
  void require_window() const {
    if (!window)
      throw std::runtime_error("reference window not initialized");
  }
  void require_shader() const {
    require_window();
    if (!loaded)
      throw std::runtime_error("reference shader not loaded");
  }
  int location(rust::Str name) const {
    require_shader();
    auto found = locations.find(text(name));
    if (found == locations.end())
      throw std::runtime_error("unknown reference uniform");
    return found->second;
  }
  ~State() {
    if (library) {
      bool retained = false;
      if (!images.empty()) {
        auto call =
            reinterpret_cast<void (*)(Image)>(dlsym(library, "UnloadImage"));
        if (call)
          for (auto &image : images)
            call(image.second);
        else
          retained = true;
      }
      // Cleanup must never throw across CXX destruction, including partial
      // libraries.
      if (loaded) {
        auto call =
            reinterpret_cast<void (*)(Shader)>(dlsym(library, "UnloadShader"));
        if (call)
          call(shader);
        else
          retained = true;
      }
      if (window) {
        auto call = reinterpret_cast<void (*)()>(dlsym(library, "CloseWindow"));
        if (call)
          call();
        else
          retained = true;
      }
      if (!retained)
        dlclose(library);
    }
  }
};
PolygonReferenceApi::PolygonReferenceApi(
    rust::Slice<const std::uint8_t> library)
    : state(std::make_unique<State>()) {
  state->library = dlopen(bytes(library).c_str(), RTLD_NOW | RTLD_LOCAL);
  if (!state->library)
    throw std::runtime_error(dlerror());
}
PolygonReferenceApi::~PolygonReferenceApi() = default;
std::unique_ptr<PolygonReferenceApi>
polygon_reference_api(rust::Slice<const std::uint8_t> library) {
  return std::make_unique<PolygonReferenceApi>(library);
}
void PolygonReferenceApi::polygon_flags(std::uint32_t flags) {
  if (state->window)
    throw std::runtime_error("reference window already initialized");
  state->symbol<void (*)(unsigned int)>("SetConfigFlags")(flags);
}
void PolygonReferenceApi::polygon_initialize(std::int32_t width,
                                             std::int32_t height,
                                             rust::Str title) {
  if (state->window)
    throw std::runtime_error("reference window already initialized");
  auto name = text(title);
  state->symbol<void (*)(int, int, const char *)>("InitWindow")(width, height,
                                                                name.c_str());
  state->window = true;
}
void PolygonReferenceApi::polygon_fps(std::int32_t fps) {
  state->require_window();
  state->symbol<void (*)(int)>("SetTargetFPS")(fps);
}
void PolygonReferenceApi::polygon_begin() {
  state->require_window();
  if (state->frame)
    throw std::runtime_error("reference frame already active");
  state->symbol<void (*)()>("BeginDrawing")();
  state->frame = true;
}
void PolygonReferenceApi::polygon_clear(std::uint32_t value) {
  state->require_window();
  if (!state->frame)
    throw std::runtime_error("reference clear outside frame");
  state->symbol<void (*)(Color)>("ClearBackground")(color(value));
}
void PolygonReferenceApi::polygon_load_shader(rust::Str vertex,
                                              rust::Str fragment) {
  state->require_window();
  if (state->loaded)
    throw std::runtime_error("reference shader already loaded");
  auto v = text(vertex), f = text(fragment);
  state->shader = state->symbol<Shader (*)(const char *, const char *)>(
      "LoadShaderFromMemory")(v.c_str(), f.c_str());
  state->loaded = true;
}
void PolygonReferenceApi::polygon_location(rust::Str name) {
  state->require_shader();
  auto key = text(name);
  state->locations[key] = state->symbol<int (*)(Shader, const char *)>(
      "GetShaderLocation")(state->shader, key.c_str());
}
void PolygonReferenceApi::polygon_integer(rust::Str name, std::int32_t value) {
  int location = state->location(name);
  state->symbol<void (*)(Shader, int, const void *, int)>("SetShaderValue")(
      state->shader, location, &value, SHADER_UNIFORM_INT);
}
void PolygonReferenceApi::polygon_floats(rust::Str name,
                                         rust::Slice<const float> values,
                                         std::uint32_t components,
                                         bool vector) {
  if ((components != 1 && components != 2 && components != 4) ||
      values.size() % components ||
      values.size() / components > std::numeric_limits<int>::max())
    throw std::runtime_error("invalid reference uniform length");
  int location = state->location(name);
  int kind = components == 1   ? SHADER_UNIFORM_FLOAT
             : components == 2 ? SHADER_UNIFORM_VEC2
                               : SHADER_UNIFORM_VEC4;
  if (!vector && values.size() != components)
    throw std::runtime_error("invalid single reference uniform length");
  if (!vector)
    state->symbol<void (*)(Shader, int, const void *, int)>("SetShaderValue")(
        state->shader, location, values.data(), kind);
  else
    state->symbol<void (*)(Shader, int, const void *, int, int)>(
        "SetShaderValueV")(state->shader, location, values.data(), kind,
                           values.size() / components);
}
void PolygonReferenceApi::polygon_matrix(rust::Str name,
                                         rust::Slice<const float> values) {
  if (values.size() != 16)
    throw std::runtime_error("invalid reference matrix length");
  Matrix m{};
  m.m0 = values[0];
  m.m1 = values[1];
  m.m2 = values[2];
  m.m3 = values[3];
  m.m4 = values[4];
  m.m5 = values[5];
  m.m6 = values[6];
  m.m7 = values[7];
  m.m8 = values[8];
  m.m9 = values[9];
  m.m10 = values[10];
  m.m11 = values[11];
  m.m12 = values[12];
  m.m13 = values[13];
  m.m14 = values[14];
  m.m15 = values[15];
  state->symbol<void (*)(Shader, int, Matrix)>("SetShaderValueMatrix")(
      state->shader, state->location(name), m);
}
void PolygonReferenceApi::polygon_begin_shader() {
  state->require_shader();
  if (!state->frame || state->mode)
    throw std::runtime_error("invalid reference shader scope");
  state->symbol<void (*)(Shader)>("BeginShaderMode")(state->shader);
  state->mode = true;
}
void PolygonReferenceApi::polygon_strip(rust::Slice<const float> points,
                                        std::uint32_t value) {
  state->require_window();
  if (!state->frame || points.size() % 2 ||
      points.size() / 2 > std::numeric_limits<int>::max())
    throw std::runtime_error("invalid reference triangle strip");
  std::vector<Vector2> vertices;
  for (size_t i = 0; i < points.size(); i += 2)
    vertices.push_back({points[i], points[i + 1]});
  state->symbol<void (*)(const Vector2 *, int, Color)>("DrawTriangleStrip")(
      vertices.data(), vertices.size(), color(value));
}
void PolygonReferenceApi::polygon_end_shader() {
  state->require_shader();
  if (!state->mode)
    throw std::runtime_error("reference shader scope not active");
  state->symbol<void (*)()>("EndShaderMode")();
  state->mode = false;
}
void PolygonReferenceApi::polygon_flush() {
  state->require_window();
  if (!state->frame)
    throw std::runtime_error("reference flush outside frame");
  state->symbol<void (*)()>("rlDrawRenderBatchActive")();
}
std::uint64_t PolygonReferenceApi::polygon_load_capture() {
  state->require_window();
  if (!state->frame)
    throw std::runtime_error("reference capture outside frame");
  if (!state->next_image)
    throw std::runtime_error("reference image token overflow");
  auto image = state->symbol<Image (*)()>("LoadImageFromScreen")();
  auto token = state->next_image++;
  state->images.emplace(token, image);
  return token;
}
bool PolygonReferenceApi::polygon_export_capture(
    std::uint64_t token, rust::Slice<const std::uint8_t> path) {
  auto found = state->images.find(token);
  if (found == state->images.end())
    throw std::runtime_error("unknown reference image token");
  auto name = bytes(path);
  return state->symbol<bool (*)(Image, const char *)>("ExportImage")(
      found->second, name.c_str());
}
void PolygonReferenceApi::polygon_unload_capture(std::uint64_t token) {
  auto found = state->images.find(token);
  if (found == state->images.end())
    throw std::runtime_error("unknown reference image token");
  state->symbol<void (*)(Image)>("UnloadImage")(found->second);
  state->images.erase(found);
}
void PolygonReferenceApi::polygon_end() {
  state->require_window();
  if (!state->frame || state->mode)
    throw std::runtime_error("invalid reference end frame");
  state->symbol<void (*)()>("EndDrawing")();
  state->frame = false;
}
void PolygonReferenceApi::polygon_unload_shader() {
  state->require_shader();
  if (state->mode)
    throw std::runtime_error("reference shader is active");
  state->symbol<void (*)(Shader)>("UnloadShader")(state->shader);
  state->loaded = false;
  state->locations.clear();
}
void PolygonReferenceApi::polygon_close() {
  state->require_window();
  if (state->frame || state->loaded || !state->images.empty())
    throw std::runtime_error("reference resources still active");
  state->symbol<void (*)()>("CloseWindow")();
  state->window = false;
}
} // namespace startup_ui

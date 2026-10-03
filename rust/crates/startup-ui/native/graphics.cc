#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include <limits>
#include <stdexcept>
#include <string>
namespace startup_ui {
uint32_t Surface::shader_load(rust::Str vertex, rust::Str fragment) {
  auto shader=LoadShaderFromMemory(std::string(vertex).c_str(),std::string(fragment).c_str());
  if (!shader.id) throw std::runtime_error("shader compilation failed");
  shaders.push_back(shader);
  return shaders.size()-1;
}
void Surface::shader_unload(uint32_t id) {
  auto &shader=shaders.at(id);
  if (shader.id) UnloadShader(shader);
  shader={};
}
void Surface::uniform_floats(uint32_t id, rust::Str name, rust::Slice<const float> values, int32_t kind, int32_t count) {
  int components=kind==SHADER_UNIFORM_FLOAT ? 1 : kind==SHADER_UNIFORM_VEC2 ? 2 : kind==SHADER_UNIFORM_VEC4 ? 4 : 0;
  if (!components || count<0 || uint64_t(count)*components!=values.size()) throw std::runtime_error("invalid shader float uniform length");
  auto shader=shaders.at(id);
  if (!shader.id) throw std::runtime_error("shader already released");
  SetShaderValueV(shader,GetShaderLocation(shader,std::string(name).c_str()),values.data(),kind,count);
}
void Surface::uniform_int(uint32_t id, rust::Str name, int32_t value) {
  auto shader=shaders.at(id);
  if (!shader.id) throw std::runtime_error("shader already released");
  SetShaderValue(shader,GetShaderLocation(shader,std::string(name).c_str()),&value,SHADER_UNIFORM_INT);
}
void Surface::uniform_matrix(uint32_t id, rust::Str name, rust::Slice<const float> values) {
  if (values.size()!=16) throw std::runtime_error("shader matrix must have 16 elements");
  auto shader=shaders.at(id);
  if (!shader.id) throw std::runtime_error("shader already released");
  Matrix matrix{values[0],values[1],values[2],values[3],values[4],values[5],values[6],values[7],values[8],values[9],values[10],values[11],values[12],values[13],values[14],values[15]};
  SetShaderValueMatrix(shader,GetShaderLocation(shader,std::string(name).c_str()),matrix);
}
void Surface::triangle_strip(rust::Slice<const Point> points, uint32_t tint, uint32_t id, bool shaded) {
  if (points.size()>std::numeric_limits<int>::max()) throw std::runtime_error("triangle strip too large");
  std::vector<Vector2> vertices;
  vertices.reserve(points.size());
  for (const auto &point:points) vertices.push_back({point.x,point.y});
  auto shader=shaded ? shaders.at(id) : Shader{};
  if (shaded && !shader.id) throw std::runtime_error("shader already released");
  if (shaded) BeginShaderMode(shader);
  DrawTriangleStrip(vertices.data(),int(vertices.size()),{uint8_t(tint),uint8_t(tint>>8),uint8_t(tint>>16),uint8_t(tint>>24)});
  if (shaded) EndShaderMode();
}
void Surface::spline(rust::Slice<const Point> points, float thick, uint32_t color) {
  if (points.size() > std::numeric_limits<int>::max()) throw std::runtime_error("spline too large");
  std::vector<Vector2> vertices;
  vertices.reserve(points.size());
  for (const auto &point : points) vertices.push_back({point.x,point.y});
  DrawSplineLinear(vertices.data(),int(vertices.size()),thick,{uint8_t(color),uint8_t(color>>8),uint8_t(color>>16),uint8_t(color>>24)});
}
} // namespace startup_ui

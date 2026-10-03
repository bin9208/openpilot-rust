#include "bridge.h"
#include "openpilot-startup-ui/src/camera_bridge.rs.h"
#include <cmath>
#include <limits>
#include <stdexcept>
namespace startup_ui {
uint32_t Surface::plane_texture(int32_t width, int32_t height, bool chroma) {
  if (width <= 0 || height <= 0 || uint64_t(width) * height * (chroma ? 2 : 1) > std::numeric_limits<int32_t>::max())
    throw std::invalid_argument("invalid camera plane dimensions");
  ::Image image{nullptr, width, height, 1, chroma ? PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA : PIXELFORMAT_UNCOMPRESSED_GRAYSCALE};
  auto texture = LoadTextureFromImage(image);
  if (!texture.id) throw std::runtime_error("camera texture allocation failed");
  textures.push_back(texture);
  return textures.size() - 1;
}
void Surface::plane_update(uint32_t id, rust::Slice<const uint8_t> bytes) {
  const auto texture = textures.at(id);
  if (!texture.id || (texture.format != PIXELFORMAT_UNCOMPRESSED_GRAYSCALE && texture.format != PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA))
    throw std::invalid_argument("camera plane is unavailable");
  const uint64_t length = uint64_t(texture.width) * texture.height * (texture.format == PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA ? 2 : 1);
  if (bytes.size() < length) throw std::invalid_argument("camera plane upload is too short");
  UpdateTexture(texture, bytes.data());
}
uint32_t Surface::texture_native(uint32_t id) const {
  const auto texture = textures.at(id);
  if (!texture.id) throw std::invalid_argument("texture has been released");
  return texture.id;
}
void Surface::camera_texture(uint32_t shader_id, uint32_t luma, uint32_t chroma, bool external, CameraRect source, CameraRect destination) {
  const auto shader = shaders.at(shader_id);
  auto texture = textures.at(luma);
  if (!shader.id || !texture.id) throw std::invalid_argument("camera draw resource has been released");
  if (external) {
    if (!std::isfinite(source.width) || !std::isfinite(source.height) || std::fabs(source.width) < 1 || source.height < 1 || std::fabs(static_cast<double>(source.width)) > std::numeric_limits<int32_t>::max() || static_cast<double>(source.height) > std::numeric_limits<int32_t>::max())
      throw std::invalid_argument("external camera dimensions out of range");
    texture.width = static_cast<int32_t>(std::fabs(source.width));
    texture.height = static_cast<int32_t>(source.height);
  }
  const auto uv = external ? ::Texture{} : textures.at(chroma);
  if (!external && !uv.id) throw std::invalid_argument("camera chroma resource has been released");
  BeginShaderMode(shader);
  if (!external) SetShaderValueTexture(shader, GetShaderLocation(shader, "texture1"), uv);
  DrawTexturePro(texture, {source.x, source.y, source.width, source.height}, {destination.x, destination.y, destination.width, destination.height}, {0,0}, 0, WHITE);
  EndShaderMode();
}
}

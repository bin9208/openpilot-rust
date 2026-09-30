#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include "rlgl.h"
#include <mutex>
#include <stdexcept>
#include <string>
namespace startup_ui {
static std::mutex window_mutex;
static bool window_owned = false;
static ::Color color(uint32_t value) {
  return {uint8_t(value), uint8_t(value >> 8), uint8_t(value >> 16),
          uint8_t(value >> 24)};
}
static ::Rectangle rectangle(Rect value) {
  return {value.x, value.y, value.width, value.height};
}
Image::Image(const char *path) : value(LoadImage(path)) {
  if (!value.data)
    throw std::runtime_error("raylib image load failed");
}
Image::~Image() { UnloadImage(value); }
int32_t Image::width() const { return value.width; }
int32_t Image::height() const { return value.height; }
void Image::premultiply() { ImageAlphaPremultiply(&value); }
void Image::flip_horizontal() { ImageFlipHorizontal(&value); }
void Image::resize(int32_t width, int32_t height) {
  if (width > 0 && height > 0)
    ImageResize(&value, width, height);
}
Surface::Surface(int32_t width, int32_t height, rust::Str title,
                 uint32_t flags) {
  std::lock_guard<std::mutex> lock(window_mutex);
  if (window_owned)
    throw std::runtime_error("raylib window already owned");
  SetConfigFlags(flags);
  InitWindow(width, height, std::string(title).c_str());
  if (!IsWindowReady())
    throw std::runtime_error("raylib window initialization failed");
  window_owned = true;
}
Surface::~Surface() {
  std::lock_guard<std::mutex> lock(window_mutex);
  for (auto &texture : textures)
    UnloadTexture(texture);
  for (auto &font : fonts)
    UnloadFont(font);
  if (target.id)
    UnloadRenderTexture(target);
  if (IsWindowReady())
    CloseWindow();
  window_owned = false;
}
uint32_t Surface::texture(Image &image, int32_t logical_width,
                          int32_t logical_height) {
  auto value = LoadTextureFromImage(image.value);
  SetTextureFilter(value, TEXTURE_FILTER_BILINEAR);
  if (logical_width > 0 && logical_height > 0) {
    value.width = logical_width;
    value.height = logical_height;
  }
  textures.push_back(value);
  return textures.size() - 1;
}
uint32_t Surface::pixel_texture(int32_t width, int32_t height, rust::Slice<const uint8_t> rgba) {
  if (width <= 0 || height <= 0 || uint64_t(width)*uint64_t(height)*4 != rgba.size())
    throw std::runtime_error("invalid RGBA texture dimensions");
  ::Image image{const_cast<uint8_t *>(rgba.data()), width, height, 1, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
  auto value=LoadTextureFromImage(image);
  if (!value.id) throw std::runtime_error("RGBA texture upload failed");
  textures.push_back(value);
  return textures.size()-1;
}
uint32_t Surface::font(rust::Str path, int32_t size,
                       rust::Slice<const int32_t> points, bool atlas,
                       bool mipmaps) {
  std::vector<int> owned(points.begin(), points.end());
  auto value = atlas ? LoadFont(std::string(path).c_str())
                     : LoadFontEx(std::string(path).c_str(), size, owned.data(),
                                  owned.size());
  if (mipmaps && value.texture.id) {
    GenTextureMipmaps(&value.texture);
    SetTextureFilter(value.texture, TEXTURE_FILTER_TRILINEAR);
  }
  fonts.push_back(value);
  return fonts.size() - 1;
}
Point Surface::measure(uint32_t font, rust::Str text, float size,
                       float spacing) const {
  auto value =
      MeasureTextEx(fonts.at(font), std::string(text).c_str(), size, spacing);
  return {value.x, value.y};
}
void Surface::text(uint32_t font, rust::Str text, Point position, float size,
                   float spacing, uint32_t tint) {
  DrawTextEx(fonts.at(font), std::string(text).c_str(),
             {position.x, position.y}, size, spacing, color(tint));
}
void Surface::draw_texture(uint32_t texture, Rect rect, Point origin,
                           float rotation) {
  auto value = textures.at(texture);
  DrawTexturePro(value, {0, 0, float(value.width), float(value.height)},
                 rectangle(rect), {origin.x, origin.y}, rotation, WHITE);
}
void Surface::tinted_texture(uint32_t texture, Rect source, Rect destination, Point origin, float rotation, uint32_t tint) {
  DrawTexturePro(textures.at(texture), rectangle(source), rectangle(destination), {origin.x,origin.y}, rotation, color(tint));
}
void Surface::circle(Point center, float radius, uint32_t tint) { DrawCircleV({center.x,center.y},radius,color(tint)); }
void Surface::gradient(Rect rect, uint32_t top_left, uint32_t bottom_left, uint32_t top_right, uint32_t bottom_right) {
  DrawRectangleGradientEx(rectangle(rect), color(top_left), color(bottom_left), color(top_right), color(bottom_right));
}
void Surface::line(Point start, Point end, float thick, uint32_t tint) { DrawLineEx({start.x,start.y},{end.x,end.y},thick,color(tint)); }
void Surface::rounded(Rect rect, float roundness, uint32_t tint, bool border) {
  if (border)
    DrawRectangleRoundedLinesEx(rectangle(rect), roundness, 10, 2, color(tint));
  else
    DrawRectangleRounded(rectangle(rect), roundness, 10, color(tint));
}
void Surface::scissor(Rect rect, bool enabled) {
  if (enabled)
    BeginScissorMode(int(rect.x), int(rect.y), int(rect.width),
                     int(rect.height));
  else
    EndScissorMode();
}
void Surface::render_target(int32_t width, int32_t height) {
  target = LoadRenderTexture(width, height);
  if (!target.id)
    throw std::runtime_error("raylib render texture failed");
  SetTextureFilter(target.texture, TEXTURE_FILTER_BILINEAR);
}
void Surface::begin(float scale) {
  if (target.id)
    BeginTextureMode(target);
  else
    BeginDrawing();
  ClearBackground(BLACK);
  if (scale != 1) {
    rlPushMatrix();
    rlScalef(scale, scale, 1);
  }
}
void Surface::end(float scale) {
  if (scale != 1)
    rlPopMatrix();
  if (target.id) {
    EndTextureMode();
    BeginDrawing();
    ClearBackground(BLACK);
    DrawTexturePro(
        target.texture,
        {0, 0, float(target.texture.width), -float(target.texture.height)},
        {0, 0, float(target.texture.width), float(target.texture.height)},
        {0, 0}, 0, WHITE);
  }
  EndDrawing();
}
void Surface::screenshot(rust::Str path) const {
  rlDrawRenderBatchActive();
  auto image =
      target.id ? LoadImageFromTexture(target.texture) : LoadImageFromScreen();
  if (target.id)
    ImageFlipVertical(&image);
  bool saved = ExportImage(image, std::string(path).c_str());
  UnloadImage(image);
  if (!saved)
    throw std::runtime_error("screenshot export failed");
}
bool Surface::should_close() const { return WindowShouldClose(); }
void Surface::target_fps(int32_t fps) { SetTargetFPS(fps); }
float Surface::frame_time() const { return GetFrameTime(); }
double Surface::time() const { return GetTime(); }
Sample Surface::sample(int32_t slot) const {
  auto point = GetTouchPosition(slot);
  return {point.x, point.y, IsMouseButtonDown(slot)};
}
float Surface::wheel() const { return GetMouseWheelMove(); }
void poll_input() { PollInputEvents(); }
Sample sample_input(int32_t slot) {
  auto point = GetTouchPosition(slot);
  return {point.x, point.y, IsMouseButtonDown(slot)};
}
std::unique_ptr<Surface> create(int32_t width, int32_t height, rust::Str title,
                                uint32_t flags) {
  return std::make_unique<Surface>(width, height, title, flags);
}
Point monitor() {
  std::lock_guard<std::mutex> lock(window_mutex);
  if (window_owned)
    throw std::runtime_error("raylib window already owned");
  InitWindow(1, 1, "");
  Point point{float(GetMonitorWidth(0)), float(GetMonitorHeight(0))};
  CloseWindow();
  return point;
}
std::unique_ptr<Image> image(rust::Str path) {
  return std::make_unique<Image>(std::string(path).c_str());
}
} // namespace startup_ui

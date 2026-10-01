#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include "rlgl.h"
#include <mutex>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>
namespace startup_ui {
static void log_callback(int level,const char *format,va_list arguments) noexcept {
  try {
    va_list copy;
    va_copy(copy,arguments);
    int count=vsnprintf(nullptr,0,format,copy);
    va_end(copy);
    if (count<0) {
      trace_log(level,{reinterpret_cast<const uint8_t *>(format),std::strlen(format)});
      return;
    }
    std::vector<char> text(size_t(count)+1);
    vsnprintf(text.data(),text.size(),format,arguments);
    trace_log(level,{reinterpret_cast<const uint8_t *>(text.data()),size_t(count)});
  } catch (...) {
    // External C callbacks must not propagate C++ exceptions through raylib.
    std::fputs("raylib log formatting failed\n",stderr);
  }
}
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
  SetTraceLogLevel(LOG_DEBUG);
  SetTraceLogCallback(log_callback);
  SetConfigFlags(flags);
  InitWindow(width, height, std::string(title).c_str());
  if (!IsWindowReady())
    throw std::runtime_error("raylib window initialization failed");
  window_owned = true;
}
Surface::~Surface() {
  std::lock_guard<std::mutex> lock(window_mutex);
  for (auto &texture : textures)
    if (texture.id) UnloadTexture(texture);
  for (auto &font : fonts)
    UnloadFont(font);
  for (auto &shader : shaders)
    if (shader.id) UnloadShader(shader);
  if (target.id)
    UnloadRenderTexture(target);
  if (shader.id)
    UnloadShader(shader);
  if (IsWindowReady())
    CloseWindow();
  window_owned = false;
}
uint32_t Surface::texture(Image &image, int32_t logical_width,
                          int32_t logical_height) {
  auto value = LoadTextureFromImage(image.value);
  SetTextureFilter(value, TEXTURE_FILTER_BILINEAR);
  SetTextureWrap(value, TEXTURE_WRAP_CLAMP);
  if (logical_width > 0 && logical_height > 0) {
    value.width = logical_width;
    value.height = logical_height;
  }
  textures.push_back(value);
  return textures.size() - 1;
}
void Surface::clear(uint32_t tint) noexcept { ClearBackground(color(tint)); }
void Surface::texture_release(uint32_t index) noexcept {
  if (index < textures.size() && textures[index].id) {
    UnloadTexture(textures[index]);
    textures[index] = {};
  }
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
void Surface::circle_gradient(Point center, float radius, uint32_t inner, uint32_t outer) { DrawCircleGradient({center.x,center.y},radius,color(inner),color(outer)); }
void Surface::gradient(Rect rect, uint32_t top_left, uint32_t bottom_left, uint32_t top_right, uint32_t bottom_right) {
  DrawRectangleGradientEx(rectangle(rect), color(top_left), color(bottom_left), color(top_right), color(bottom_right));
}
void Surface::line(Point start, Point end, float thick, uint32_t tint) { DrawLineEx({start.x,start.y},{end.x,end.y},thick,color(tint)); }
void Surface::rounded_segments(Rect rect, float roundness, int32_t segments, uint32_t tint, bool border) {
  if (border) DrawRectangleRoundedLinesEx(rectangle(rect), roundness, segments, 2, color(tint));
  else DrawRectangleRounded(rectangle(rect), roundness, segments, color(tint));
}
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
  if (target.id) return;
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
void Surface::finish_content(float scale) {
  if (scale != 1)
    rlPopMatrix();
  if (target.id) {
    EndTextureMode();
    BeginDrawing();
    ClearBackground(BLACK);
    if (shader.id) BeginShaderMode(shader);
    DrawTexturePro(
        target.texture,
        {0, 0, float(target.texture.width), -float(target.texture.height)},
        {0, 0, float(target.texture.width), float(target.texture.height)},
        {0, 0}, 0, WHITE);
    if (shader.id) EndShaderMode();
  }
}
void Surface::present() { EndDrawing(); }
void Surface::end(float scale) { finish_content(scale); present(); }
bool Surface::has_render_target() const { return target.id != 0; }
void Surface::burn_in(rust::Str vertex, rust::Str fragment) {
  if (shader.id) return;
  shader = LoadShaderFromMemory(std::string(vertex).c_str(), std::string(fragment).c_str());
  if (!shader.id) throw std::runtime_error("burn-in shader failed");
}
rust::Vec<uint8_t> Surface::capture_pixels() const {
  if (!target.id) throw std::runtime_error("recording render target missing");
  rlDrawRenderBatchActive();
  auto image = LoadImageFromTexture(target.texture);
  if (!image.data) throw std::runtime_error("recording texture readback failed");
  struct ImageGuard { ::Image value; ~ImageGuard() { UnloadImage(value); } } guard{image};
  if (image.format != PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 || image.width <= 0 || image.height <= 0)
    throw std::runtime_error("recording image is not RGBA");
  auto count=uint64_t(image.width)*uint64_t(image.height)*4;
  rust::Vec<uint8_t> bytes;
  bytes.reserve(count);
  const auto *pixels=static_cast<const uint8_t *>(image.data);
  for (uint64_t i=0; i<count; ++i) bytes.push_back(pixels[i]);
  return bytes;
}
void Surface::set_title(rust::Str title) { SetWindowTitle(std::string(title).c_str()); }
int32_t Surface::fps() const { return GetFPS(); }
void Surface::draw_fps(int32_t x, int32_t y) { DrawFPS(x,y); }
int32_t Surface::key_pressed() const { return GetKeyPressed(); }
int32_t Surface::char_pressed() const { return GetCharPressed(); }
bool Surface::key_down(int32_t key) const { return IsKeyDown(key); }
bool Surface::key_started(int32_t key) const { return IsKeyPressed(key); }
Point Surface::mouse_position() const { auto position=GetMousePosition();return {position.x,position.y}; }

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
void Surface::screen_screenshot(rust::Str path) const {
  rlDrawRenderBatchActive();auto image=LoadImageFromScreen();bool saved=ExportImage(image,std::string(path).c_str());UnloadImage(image);if(!saved)throw std::runtime_error("screen export failed");
}
void Surface::rectangle_lines(int32_t x,int32_t y,int32_t width,int32_t height,uint32_t tint){DrawRectangleLines(x,y,width,height,color(tint));}
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

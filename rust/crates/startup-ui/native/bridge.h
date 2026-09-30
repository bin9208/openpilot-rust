#pragma once
#include "raylib.h"
#include "rust/cxx.h"
#include <memory>
#include <vector>
namespace startup_ui {
struct Point;
struct Rect;
struct Sample;
class Image {
public:
  explicit Image(const char *path);
  ~Image();
  int32_t width() const;
  int32_t height() const;
  void premultiply();
  void flip_horizontal();
  void resize(int32_t width, int32_t height);
  ::Image value;
};
class Surface {
public:
  Surface(int32_t width, int32_t height, rust::Str title, uint32_t flags);
  ~Surface();
  uint32_t texture(Image &image, int32_t logical_width, int32_t logical_height);
  uint32_t pixel_texture(int32_t width, int32_t height, rust::Slice<const uint8_t> rgba);
  uint32_t font(rust::Str path, int32_t size, rust::Slice<const int32_t> points,
                bool atlas, bool mipmaps);
  Point measure(uint32_t font, rust::Str text, float size, float spacing) const;
  void text(uint32_t font, rust::Str text, Point position, float size,
            float spacing, uint32_t color);
  void draw_texture(uint32_t texture, Rect rect, Point origin, float rotation);
  void tinted_texture(uint32_t texture, Rect source, Rect destination, Point origin, float rotation, uint32_t tint);
  void circle(Point center, float radius, uint32_t color);
  void gradient(Rect rect, uint32_t top_left, uint32_t bottom_left, uint32_t top_right, uint32_t bottom_right);
  void line(Point start, Point end, float thick, uint32_t color);
  void rounded(Rect rect, float roundness, uint32_t color, bool border);
  void scissor(Rect rect, bool enabled);
  void render_target(int32_t width, int32_t height);
  void begin(float scale);
  void end(float scale);
  void screenshot(rust::Str path) const;
  bool should_close() const;
  void target_fps(int32_t fps);
  float frame_time() const;
  double time() const;
  Sample sample(int32_t slot) const;
  float wheel() const;

private:
  std::vector<::Font> fonts;
  std::vector<::Texture> textures;
  ::RenderTexture target{};
};
std::unique_ptr<Surface> create(int32_t width, int32_t height, rust::Str title,
                                uint32_t flags);
Point monitor();
void poll_input();
Sample sample_input(int32_t slot);
std::unique_ptr<Image> image(rust::Str path);
} // namespace startup_ui

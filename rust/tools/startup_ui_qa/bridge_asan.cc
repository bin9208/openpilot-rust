#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include <iostream>
#include <stdexcept>
int main(int argc, char **argv) {
  if (argc != 3)
    return 2;
  for (int i = 0; i < 3; i++) {
    auto surface = startup_ui::create(536, 240, "ASAN startup UI", 32);
    bool rejected = false;
    try {
      auto duplicate = startup_ui::create(1, 1, "duplicate", 0);
    } catch (const std::runtime_error &) {
      rejected = true;
    }
    if (!rejected)
      return 3;
    auto image = startup_ui::image(argv[1]);
    image->premultiply();
    image->flip_horizontal();
    image->resize(140, 140);
    auto texture = surface->texture(*image, 0, 0);
    std::vector<uint8_t> rgba(4 * 4 * 4, 180);
    auto pixels = surface->pixel_texture(4, 4, {rgba.data(), rgba.size()});
    bool invalid_pixels = false;
    try { surface->pixel_texture(5, 4, {rgba.data(), rgba.size()}); }
    catch (const std::runtime_error &) { invalid_pixels = true; }
    if (!invalid_pixels) return 4;
    const std::string source(argv[1]);
    const std::string font_path = source.substr(0, source.find_last_of('/')) + "/fonts/Inter-Medium.ttf";
    const std::vector<int32_t> points{32, 65, 66, 67};
    auto font = surface->font(font_path, 48, {points.data(), points.size()}, false, true);
    if (surface->measure(font, "ABC", 30, 0).x <= 0) return 5;
    surface->begin(1);
    surface->text(font, "ABC", {12, 12}, 30, 0, 0xffffffff);
    surface->circle({40, 180}, 20, 0xff00ff00);
    surface->gradient({90, 160, 80, 40}, 0xff000000, 0xff000000, 0xffffffff, 0xffffffff);
    surface->line({180, 180}, {220, 210}, 3, 0xffffffff);
    surface->tinted_texture(pixels, {0, 0, 4, 4}, {400, 160, 40, 40}, {0, 0}, 0, 0xffffffff);
    surface->draw_texture(texture, {268, 120, 140, 140}, {70, 70}, 45);
    surface->screenshot(argv[2]);
    surface->end(1);
  }
  std::cout << "PASS ASAN native bridge three create/render/capture/destroy "
               "cycles; duplicate window and invalid pixel dimensions rejected; fonts, pixels, shapes and tint exercised\n";
}

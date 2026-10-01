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
    image->resize(140, 140);
    auto texture = surface->texture(*image, 0, 0);
    surface->begin(1);
    surface->draw_texture(texture, {268, 120, 140, 140}, {70, 70}, 45);
    surface->screenshot(argv[2]);
    surface->end(1);
  }
  std::cout << "PASS ASAN native bridge three create/render/capture/destroy "
               "cycles; duplicate window rejected\n";
}

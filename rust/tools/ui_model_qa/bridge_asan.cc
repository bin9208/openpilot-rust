#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include <iostream>
#include <string>
namespace startup_ui {
void trace_log(int32_t, rust::Slice<const uint8_t>) noexcept {}
}
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  for (int cycle = 0; cycle < 8; ++cycle) {
    auto surface = startup_ui::create(536, 240, "model renderer adapter ASAN", 32);
    if (surface->measure_default("", 32) != 0) return 3;
    if (surface->measure_default("42.0", 32) <= 0) return 4;
    surface->begin(1);
    surface->clear(0xff000000);
    for (int frame = 0; frame < 128; ++frame) {
      surface->measure_default("native radar \xED\x95\x9C\xEA\xB8\x80", frame % 48 + 1);
      surface->rounded_outline({float(frame), 40, 120, 64}, 0.15f, 12, 3, 0xffffffff);
      surface->rounded_outline({280, 100, 120, 64}, 0.28f, 8, 1.5f, 0xff00ffff);
    }
    surface->screenshot(argv[1]);
    surface->finish_content(1);
    surface->present();
  }
  std::cout << "PASS 8 native create/destroy cycles; 1024 default-font Unicode measurements and 2048 rounded outlines; actual GL screenshot\n";
}

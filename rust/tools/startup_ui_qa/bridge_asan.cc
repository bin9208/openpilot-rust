#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include "openpilot-startup-ui/src/camera_bridge.rs.h"
#include <iostream>
#include <stdexcept>
#include <string>
namespace startup_ui {
static int log_count=0;
void trace_log(int32_t,rust::Slice<const uint8_t> bytes) noexcept { if (!bytes.empty()) ++log_count; }
}
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
    surface->render_target(536,240);
    surface->render_target(536,240);
    if (!surface->has_render_target()) return 6;
    const std::string vertex="#version 300 es\nprecision highp float;\nin vec3 vertexPosition;uniform mat4 mvp;void main(){gl_Position=mvp*vec4(vertexPosition,1.0);}";
    const std::string fragment="#version 300 es\nprecision highp float;\nout vec4 finalColor;uniform vec4 fillColor;void main(){finalColor=fillColor;}";
    auto shader=surface->shader_load(vertex,fragment);
    std::vector<float> matrix{2.0f/536,0,0,-1,0,-2.0f/240,0,1,0,0,-1,0,0,0,0,1};
    surface->uniform_matrix(shader,"mvp",{matrix.data(),matrix.size()});
    std::vector<float> color{0.0f,1.0f,0.0f,1.0f};
    surface->uniform_floats(shader,"fillColor",{color.data(),color.size()},3,1);
    bool rejected_uniform=false;
    try { surface->uniform_floats(shader,"fillColor",{color.data(),3},3,1); }
    catch (const std::runtime_error &) { rejected_uniform=true; }
    if (!rejected_uniform) return 7;
    surface->set_title("ASAN frame ownership");
    surface->key_pressed();surface->char_pressed();surface->key_down(257);surface->key_started(257);surface->mouse_position();surface->fps();
    surface->begin(1);
    surface->clear(0xff332211);
    const auto cleared = surface->capture_pixels();
    if (cleared.size() != 536*240*4 || cleared[0] != 0x11 || cleared[1] != 0x22 || cleared[2] != 0x33) return 10;
    const std::vector<startup_ui::Point> polygon_points{{200,20},{240,20},{220,80}};
    surface->triangle_strip({polygon_points.data(),polygon_points.size()},0xffffffff,shader,true);
    surface->circle_gradient({80,80},25,0xff0000ff,0);
    surface->text(font, "ABC", {12, 12}, 30, 0, 0xffffffff);
    surface->circle({40, 180}, 20, 0xff00ff00);
    surface->gradient({90, 160, 80, 40}, 0xff000000, 0xff000000, 0xffffffff, 0xffffffff);
    surface->line({180, 180}, {220, 210}, 3, 0xffffffff);
    surface->tinted_texture(pixels, {0, 0, 4, 4}, {400, 160, 40, 40}, {0, 0}, 0, 0xffffffff);
    const auto luma=surface->plane_texture(4,4,false);
    const auto chroma=surface->plane_texture(2,2,true);
    std::vector<uint8_t> plane(16,128);
    surface->plane_update(luma,{plane.data(),plane.size()});
    surface->plane_update(chroma,{plane.data(),8});
    bool short_plane=false;
    try { surface->plane_update(luma,{plane.data(),1}); }
    catch (const std::invalid_argument &) { short_plane=true; }
    if (!short_plane || !surface->texture_native(luma)) return 11;
    surface->camera_texture(shader,luma,chroma,false,{0,0,-4,4},{0,0,40,40});
    bool invalid_external=false;
    try { surface->camera_texture(shader,luma,0,true,{0,0,2147483648.0f,4},{0,0,40,40}); }
    catch (const std::invalid_argument &) { invalid_external=true; }
    if (!invalid_external) return 12;
    surface->texture_release(luma);
    surface->texture_release(chroma);
    bool released_plane=false;
    try { surface->plane_update(luma,{plane.data(),plane.size()}); }
    catch (const std::invalid_argument &) { released_plane=true; }
    if (!released_plane) return 13;
    surface->draw_texture(texture, {268, 120, 140, 140}, {70, 70}, 45);
    surface->screenshot(argv[2]);
    surface->finish_content(1);
    surface->draw_fps(10,10);
    surface->present();
    if (surface->capture_pixels().size()!=536*240*4) return 8;
    surface->shader_unload(shader);
    surface->shader_unload(shader);
    surface->texture_release(pixels);
    surface->texture_release(pixels);
    surface->texture_release(0xffffffff);
    bool rejected_shader=false;
    try { surface->uniform_int(shader,"missing",0); }
    catch (const std::runtime_error &) { rejected_shader=true; }
    if (!rejected_shader) return 9;
  }
  if (startup_ui::log_count==0) return 10;
  std::cout << "PASS ASAN native bridge three create/render/capture/destroy "
               "cycles; duplicate window and invalid pixel dimensions rejected; fonts, pixels, shapes and tint exercised\n";
}

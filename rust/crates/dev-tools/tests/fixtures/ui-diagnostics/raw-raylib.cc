// Raw ownership and raylib ABI only. Rust owns all scene and overlay policy.
#include "raw-raylib.h"
#include "raylib.h"
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <memory>
#include <stdexcept>
#include <string>
struct diagnostic_api {
  void *library = nullptr;
  bool window = false, frame = false, target_mode = false, shader_mode = false;
  Font font{};
  RenderTexture target{};
  Shader shader{};
  template <class T> T call(const char *name) {
    auto value = dlsym(library, name);
    if (!value)
      throw std::runtime_error(std::string("missing diagnostic ABI ") + name);
    return reinterpret_cast<T>(value);
  }
  void ready() {
    if (!window)
      throw std::runtime_error("diagnostic window missing");
  }
  ~diagnostic_api() {
    if (!library)
      return;
    if (window) {
      if (shader_mode)
        call<void (*)()>("EndShaderMode")();
      if (target_mode)
        call<void (*)()>("EndTextureMode")();
      if (frame)
        call<void (*)()>("EndDrawing")();
      if (font.texture.id)
        call<void (*)(Font)>("UnloadFont")(font);
      if (target.id)
        call<void (*)(RenderTexture)>("UnloadRenderTexture")(target);
      if (shader.id)
        call<void (*)(Shader)>("UnloadShader")(shader);
      call<void (*)()>("CloseWindow")();
    }
    dlclose(library);
  }
};
static Color color(uint32_t v) {
  return {uint8_t(v), uint8_t(v >> 8), uint8_t(v >> 16), uint8_t(v >> 24)};
}
template <class F> int guarded(char *error, size_t size, F f) {
  try {
    f();
    return 1;
  } catch (const std::exception &e) {
    if (size)
      std::snprintf(error, size, "%s", e.what());
    return 0;
  }
}
#define ARGS diagnostic_api *a, char *e, size_t n
#define DO(...) return guarded(e, n, [&] { __VA_ARGS__; })
extern "C" {
// Declarations above have C++ linkage unless included inside extern C; the
// executable adapter links these exact C symbols from a separately built file.
}
diagnostic_api *diagnostic_open(const unsigned char *bytes, size_t length,
                                char *error, size_t size) {
  std::unique_ptr<diagnostic_api> a;
  if (!guarded(error, size, [&] {
        std::string path(reinterpret_cast<const char *>(bytes), length);
        if (path.find('\0') != std::string::npos)
          throw std::runtime_error("NUL library path");
        a = std::make_unique<diagnostic_api>();
        a->library = dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
        if (!a->library)
          throw std::runtime_error(dlerror());
        // Check cleanup symbols before any allocation. Destruction cannot
        // throw.
        for (auto name :
             {"EndShaderMode", "EndTextureMode", "EndDrawing", "UnloadFont",
              "UnloadRenderTexture", "UnloadShader", "CloseWindow"})
          a->call<void (*)()>(name);
      }))
    return nullptr;
  return a.release();
}
void diagnostic_destroy(diagnostic_api *a) { delete a; }
int diagnostic_flags(diagnostic_api *a, unsigned flags, char *e, size_t n) {
  DO(a->call<void (*)(unsigned)>("SetConfigFlags")(flags));
}
int diagnostic_window(diagnostic_api *a, int w, int h, const char *title,
                      char *e, size_t n) {
  DO(if (a->window) throw std::runtime_error("duplicate window");
     a->call<void (*)(int, int, const char *)>("InitWindow")(w, h, title);
     a->window = true; a->call<void (*)(int)>("SetTargetFPS")(0));
}
int diagnostic_begin(ARGS) {
  DO(a->ready(); if (a->frame) throw std::runtime_error("duplicate frame");
     a->call<void (*)()>("BeginDrawing")(); a->frame = true);
}
int diagnostic_clear(diagnostic_api *a, uint32_t c, char *e, size_t n) {
  DO(a->ready(); a->call<void (*)(Color)>("ClearBackground")(color(c)));
}
int diagnostic_rectangle(diagnostic_api *a, float x, float y, float w, float h,
                         uint32_t c, char *e, size_t n) {
  DO(a->ready(); a->call<void (*)(Rectangle, Color)>("DrawRectangleRec")(
      {x, y, w, h}, color(c)));
}
int diagnostic_line(diagnostic_api *a, int x, int y, int x2, int y2, uint32_t c,
                    char *e, size_t n) {
  DO(a->ready(); a->call<void (*)(int, int, int, int, Color)>("DrawLine")(
      x, y, x2, y2, color(c)));
}
int diagnostic_circle(diagnostic_api *a, int x, int y, float r, uint32_t c,
                      char *e, size_t n) {
  DO(a->ready(); a->call<void (*)(int, int, float, Color)>("DrawCircle")(
      x, y, r, color(c)));
}
int diagnostic_font(diagnostic_api *a, const char *path, int size, int filter,
                    const int *points, int count, char *e, size_t n) {
  DO(a->ready();
     if (a->font.texture.id) throw std::runtime_error("duplicate font");
     if (size == 0) a->font = a->call<Font (*)(const char *)>("LoadFont")(path);
     else a->font = a->call<Font (*)(const char *, int, int *, int)>(
         "LoadFontEx")(path, size, const_cast<int *>(points), count);
     if (!a->font.texture.id) throw std::runtime_error("font load failed");
     if (filter >= 0) {
       a->call<void (*)(Texture *)>("GenTextureMipmaps")(&a->font.texture);
       a->call<void (*)(Texture, int)>("SetTextureFilter")(a->font.texture,
                                                           filter);
     });
}
int diagnostic_text(diagnostic_api *a, const char *text, float x, float y,
                    float size, uint32_t c, char *e, size_t n) {
  DO(a->ready();
     if (!a->font.texture.id) throw std::runtime_error("font missing");
     a->call<void (*)(Font, const char *, Vector2, float, float, Color)>(
         "DrawTextEx")(a->font, text, {x, y}, size, 0, color(c)));
}
int diagnostic_target(diagnostic_api *a, int w, int h, char *e, size_t n) {
  DO(a->ready(); if (a->target.id) throw std::runtime_error("duplicate target");
     a->target =
         a->call<RenderTexture (*)(int, int)>("LoadRenderTexture")(w, h);
     a->call<void (*)(Texture, int)>("SetTextureFilter")(
         a->target.texture, TEXTURE_FILTER_BILINEAR));
}
int diagnostic_target_begin(ARGS) {
  DO(a->ready(); if (!a->target.id || a->target_mode) throw std::runtime_error(
         "invalid target scope");
     a->call<void (*)(RenderTexture)>("BeginTextureMode")(a->target);
     a->target_mode = true);
}
int diagnostic_target_end(ARGS) {
  DO(if (!a->target_mode) throw std::runtime_error("target scope missing");
     a->call<void (*)()>("EndTextureMode")(); a->target_mode = false);
}
int diagnostic_shader(diagnostic_api *a, const char *v, const char *f, char *e,
                      size_t n) {
  DO(a->ready(); if (a->shader.id) throw std::runtime_error("duplicate shader");
     a->shader = a->call<Shader (*)(const char *, const char *)>(
         "LoadShaderFromMemory")(v, f));
}
int diagnostic_shader_begin(ARGS) {
  DO(a->ready();
     if (!a->shader.id || a->shader_mode ||
         !a->frame) throw std::runtime_error("invalid shader scope");
     a->call<void (*)(Shader)>("BeginShaderMode")(a->shader);
     a->shader_mode = true);
}
int diagnostic_shader_end(ARGS) {
  DO(if (!a->shader_mode) throw std::runtime_error("shader scope missing");
     a->call<void (*)()>("EndShaderMode")(); a->shader_mode = false);
}
int diagnostic_target_draw(ARGS) {
  DO(a->ready(); if (!a->target.id ||
                     !a->frame) throw std::runtime_error("target draw invalid");
     a->call<void (*)(Texture, Rectangle, Rectangle, Vector2, float, Color)>(
         "DrawTexturePro")(a->target.texture,
                           {0, 0, float(a->target.texture.width),
                            -float(a->target.texture.height)},
                           {0, 0, float(a->target.texture.width),
                            float(a->target.texture.height)},
                           {0, 0}, 0, color(0xffffffff)));
}
int diagnostic_export(diagnostic_api *a, const char *path, char *e, size_t n) {
  DO(a->ready();
     if (!a->frame) throw std::runtime_error("capture outside frame");
     auto flush = a->call<void (*)()>("rlDrawRenderBatchActive");
     auto load = a->call<Image (*)()>("LoadImageFromScreen");
     auto export_image = a->call<bool (*)(Image, const char *)>("ExportImage");
     auto unload = a->call<void (*)(Image)>("UnloadImage");
     flush(); auto image = load();
     bool ok = export_image(image, path);
     unload(image);
     if (!ok) throw std::runtime_error("diagnostic export failed"));
}
int diagnostic_end(ARGS) {
  DO(if (!a->frame ||
         a->shader_mode) throw std::runtime_error("frame scope invalid");
     a->call<void (*)()>("EndDrawing")(); a->frame = false);
}

#include <iostream>
#include <sstream>
#include <unistd.h>
#include <vector>
static std::string unhex(const std::string &s) {
  if (s == "-")
    return "";
  if (s.size() % 2)
    throw std::runtime_error("invalid string length");
  std::string out;
  for (size_t i = 0; i < s.size(); i += 2) {
    unsigned v = 0;
    for (size_t j = 0; j < 2; j++) {
      char c = s[i + j];
      if (c >= '0' && c <= '9')
        v = v * 16 + c - '0';
      else if (c >= 'a' && c <= 'f')
        v = v * 16 + c - 'a' + 10;
      else
        throw std::runtime_error("invalid string hex");
    }
    if (!v)
      throw std::runtime_error("NUL raw string");
    out.push_back(char(v));
  }
  return out;
}
static std::string hex(const char *s) {
  const char *digits = "0123456789abcdef";
  std::string out;
  for (; *s; s++) {
    unsigned c = static_cast<unsigned char>(*s);
    out.push_back(digits[c >> 4]);
    out.push_back(digits[c & 15]);
  }
  return out;
}
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  // Protocol gets a private duplicate; raylib's stdout diagnostics go to
  // stderr.
  int descriptor = dup(STDOUT_FILENO);
  if (descriptor < 0)
    return 2;
  FILE *protocol = fdopen(descriptor, "w");
  if (!protocol)
    return 2;
  if (dup2(STDERR_FILENO, STDOUT_FILENO) < 0)
    return 2;
  char error[1024] = {0};
  std::unique_ptr<diagnostic_api> api(
      diagnostic_open(reinterpret_cast<const unsigned char *>(argv[1]),
                      std::strlen(argv[1]), error, sizeof(error)));
  if (!api) {
    std::fprintf(protocol, "ERR %s\n", hex(error).c_str());
    std::fflush(protocol);
    return 1;
  }
  std::fprintf(protocol, "OK\n");
  std::fflush(protocol);
  for (std::string line; std::getline(std::cin, line);) {
    try {
      std::istringstream input(line);
      std::string op;
      input >> op;
      int ok = 0;
      error[0] = 0;
      auto fields = [&] {
        if (!input)
          throw std::runtime_error("truncated raw command");
        std::string extra;
        if (input >> extra)
          throw std::runtime_error("extra raw command fields");
      };
      if (op == "quit") {
        fields();
        api.reset();
        std::fprintf(protocol, "OK\n");
        std::fflush(protocol);
        return 0;
      }
      if (op == "flags") {
        unsigned flags = 0;
        input >> flags;
        fields();
        ok = diagnostic_flags(api.get(), flags, error, sizeof(error));
      } else if (op == "window") {
        int w = 0, h = 0;
        std::string title;
        input >> w >> h >> title;
        fields();
        ok = diagnostic_window(api.get(), w, h, unhex(title).c_str(), error,
                               sizeof(error));
      } else if (op == "begin") {
        fields();
        ok = diagnostic_begin(api.get(), error, sizeof(error));
      } else if (op == "clear") {
        uint32_t c = 0;
        input >> c;
        fields();
        ok = diagnostic_clear(api.get(), c, error, sizeof(error));
      } else if (op == "rect") {
        float x = 0, y = 0, w = 0, h = 0;
        uint32_t c = 0;
        input >> x >> y >> w >> h >> c;
        fields();
        ok = diagnostic_rectangle(api.get(), x, y, w, h, c, error,
                                  sizeof(error));
      } else if (op == "line") {
        int x = 0, y = 0, x2 = 0, y2 = 0;
        uint32_t c = 0;
        input >> x >> y >> x2 >> y2 >> c;
        fields();
        ok = diagnostic_line(api.get(), x, y, x2, y2, c, error, sizeof(error));
      } else if (op == "circle") {
        int x = 0, y = 0;
        float r = 0;
        uint32_t c = 0;
        input >> x >> y >> r >> c;
        fields();
        ok = diagnostic_circle(api.get(), x, y, r, c, error, sizeof(error));
      } else if (op == "font") {
        std::string path;
        int size = 0, filter = 0, count = 0;
        input >> path >> size >> filter >> count;
        if (count < 0 || count > 0x110000)
          throw std::runtime_error("font codepoint count invalid");
        std::vector<int> points(static_cast<size_t>(count));
        for (int &point : points)
          input >> point;
        fields();
        ok = diagnostic_font(api.get(), unhex(path).c_str(), size, filter,
                             points.data(), count, error, sizeof(error));
      } else if (op == "text") {
        std::string text;
        float x = 0, y = 0, size = 0;
        uint32_t c = 0;
        input >> text >> x >> y >> size >> c;
        fields();
        ok = diagnostic_text(api.get(), unhex(text).c_str(), x, y, size, c,
                             error, sizeof(error));
      } else if (op == "target") {
        int w = 0, h = 0;
        input >> w >> h;
        fields();
        ok = diagnostic_target(api.get(), w, h, error, sizeof(error));
      } else if (op == "target-begin") {
        fields();
        ok = diagnostic_target_begin(api.get(), error, sizeof(error));
      } else if (op == "target-end") {
        fields();
        ok = diagnostic_target_end(api.get(), error, sizeof(error));
      } else if (op == "shader") {
        std::string v, f;
        input >> v >> f;
        fields();
        ok = diagnostic_shader(api.get(), unhex(v).c_str(), unhex(f).c_str(),
                               error, sizeof(error));
      } else if (op == "shader-begin") {
        fields();
        ok = diagnostic_shader_begin(api.get(), error, sizeof(error));
      } else if (op == "shader-end") {
        fields();
        ok = diagnostic_shader_end(api.get(), error, sizeof(error));
      } else if (op == "target-draw") {
        fields();
        ok = diagnostic_target_draw(api.get(), error, sizeof(error));
      } else if (op == "export") {
        std::string path;
        input >> path;
        fields();
        ok = diagnostic_export(api.get(), unhex(path).c_str(), error,
                               sizeof(error));
      } else if (op == "end") {
        fields();
        ok = diagnostic_end(api.get(), error, sizeof(error));
      } else
        throw std::runtime_error("unknown raw command");
      if (!ok)
        throw std::runtime_error(error);
      std::fprintf(protocol, "OK\n");
    } catch (const std::exception &e) {
      std::fprintf(protocol, "ERR %s\n", hex(e.what()).c_str());
    }
    std::fflush(protocol);
  }
}

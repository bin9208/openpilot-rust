// Owned raylib resources and mechanical ABI only; no widget policy.
#include "raylib.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unistd.h>
#include <vector>
// Match Python CFFI's binary64 -> binary32 argument conversion. Parsing a
// rounded decimal directly into float can select the other neighbor at ties.
struct AbiFloat {
  float value;
  AbiFloat(float v = 0) : value(v) {}
  operator float() const { return value; }
  friend std::istream &operator>>(std::istream &in, AbiFloat &out) {
    double value = 0;
    in >> value;
    out.value = static_cast<float>(value);
    return in;
  }
};
struct Api {
  void *library = nullptr;
  bool window = false, frame = false, clip = false, texture_frame = false,
       matrix = false, fallback = false;
  RenderTexture target{};
  Shader shader{};
  Font font{};
  std::vector<Font> fonts;
  std::vector<Texture> textures;
  std::vector<Texture> emoji;
  template <class T> T get(const char *name) {
    void *p = dlsym(library, name);
    if (!p)
      throw std::runtime_error(std::string("missing forms ABI ") + name);
    return reinterpret_cast<T>(p);
  }
  ~Api() {
    if (!library)
      return;
    if (window) {
      if (clip)
        get<void (*)()>("EndScissorMode")();
      if (matrix)
        get<void (*)()>("rlPopMatrix")();
      if (texture_frame)
        get<void (*)()>("EndTextureMode")();
      if (frame)
        get<void (*)()>("EndDrawing")();
      if (shader.id)
        get<void (*)(Shader)>("UnloadShader")(shader);
      if (target.id)
        get<void (*)(RenderTexture)>("UnloadRenderTexture")(target);
      for (auto texture : emoji)
        if (texture.id)
          get<void (*)(Texture)>("UnloadTexture")(texture);
      for (auto texture : textures)
        if (texture.id)
          get<void (*)(Texture)>("UnloadTexture")(texture);
      for (auto item : fonts)
        if (item.texture.id)
          get<void (*)(Font)>("UnloadFont")(item);
      get<void (*)()>("CloseWindow")();
    }
    dlclose(library);
  }
};
std::string unhex(const std::string &s) {
  if (s == "-")
    return "";
  if (s.size() % 2)
    throw std::runtime_error("invalid forms hex");
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
        throw std::runtime_error("invalid forms hex");
    }
    if (!v)
      throw std::runtime_error("NUL forms string");
    out.push_back(char(v));
  }
  return out;
}
std::string hex(const char *s) {
  const char *d = "0123456789abcdef";
  std::string out;
  for (; *s; s++) {
    unsigned c = static_cast<unsigned char>(*s);
    out += d[c >> 4];
    out += d[c & 15];
  }
  return out;
}
Color color(uint32_t n) {
  return {uint8_t(n), uint8_t(n >> 8), uint8_t(n >> 16), uint8_t(n >> 24)};
}
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  int fd = dup(STDOUT_FILENO);
  if (fd < 0)
    return 2;
  FILE *reply = fdopen(fd, "w");
  if (!reply)
    return 2;
  if (dup2(STDERR_FILENO, STDOUT_FILENO) < 0)
    return 2;
  std::unique_ptr<Api> a;
  try {
    a = std::make_unique<Api>();
    a->library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!a->library)
      throw std::runtime_error(dlerror());
    for (auto name : {"EndScissorMode", "EndDrawing", "UnloadFont",
                      "UnloadTexture", "CloseWindow", "UnloadRenderTexture",
                      "UnloadShader", "EndTextureMode", "rlPopMatrix"})
      a->get<void (*)()>(name);
    std::fprintf(reply, "OK\n");
    std::fflush(reply);
  } catch (const std::exception &e) {
    std::fprintf(reply, "ERR %s\n", hex(e.what()).c_str());
    std::fflush(reply);
    return 1;
  }
  for (std::string line; std::getline(std::cin, line);) {
    try {
      std::istringstream in(line);
      std::string op;
      in >> op;
      std::string result;
      auto done = [&] {
        if (!in)
          throw std::runtime_error("truncated forms command");
        std::string extra;
        if (in >> extra)
          throw std::runtime_error("extra forms fields");
      };
      auto ready = [&] {
        if (!a->window)
          throw std::runtime_error("forms window missing");
      };
      if (op == "quit") {
        done();
        a.reset();
        std::fprintf(reply, "OK\n");
        std::fflush(reply);
        return 0;
      } else if (op == "flags") {
        unsigned flags = 0;
        in >> flags;
        done();
        a->get<void (*)(unsigned)>("SetConfigFlags")(flags);
      } else if (op == "window") {
        int w = 0, h = 0;
        std::string title;
        in >> w >> h >> title;
        done();
        if (a->window)
          throw std::runtime_error("duplicate forms window");
        a->get<void (*)(int, int, const char *)>("InitWindow")(
            w, h, unhex(title).c_str());
        if (!a->get<bool (*)()>("IsWindowReady")())
          throw std::runtime_error("forms window initialization failed");
        a->window = true;
        a->get<void (*)(int)>("SetTargetFPS")(0);
      } else if (op == "font" || op == "font-load" || op == "font-atlas" ||
                 op == "font-load-plain" || op == "font-atlas-plain") {
        std::string path;
        int size = 0, count = 0;
        size_t identifier = 0;
        if (op != "font")
          in >> identifier;
        in >> path >> size >> count;
        if (count < 0 || count > 0x110000)
          throw std::runtime_error("forms codepoint count");
        std::vector<int> points(static_cast<size_t>(count));
        for (int &point : points)
          in >> point;
        done();
        ready();
        if (identifier != a->fonts.size())
          throw std::runtime_error("invalid forms font identifier");
        if (op == "font-atlas" || op == "font-atlas-plain")
          a->font = a->get<Font (*)(const char *)>("LoadFont")(unhex(path).c_str());
        else
          a->font = a->get<Font (*)(const char *, int, int *, int)>("LoadFontEx")(
              unhex(path).c_str(), size, points.data(), count);
        a->fonts.push_back(a->font);
        if (!a->font.texture.id)
          throw std::runtime_error("forms font load failed");
        if (op != "font-load-plain" && op != "font-atlas-plain") {
          a->get<void (*)(Texture *)>("GenTextureMipmaps")(&a->font.texture);
          a->get<void (*)(Texture, int)>("SetTextureFilter")(
              a->font.texture, TEXTURE_FILTER_TRILINEAR);
        }
      } else if (op == "font-fallback") {
        done();
        a->fallback = true;
      } else if (op == "font-mipmaps") {
        size_t identifier = 0;
        in >> identifier;
        done();
        ready();
        if (identifier >= a->fonts.size())
          throw std::runtime_error("original font missing");
        Font &font = a->fonts[identifier];
        a->get<void (*)(Texture *)>("GenTextureMipmaps")(&font.texture);
        a->get<void (*)(Texture, int)>("SetTextureFilter")(
            font.texture, TEXTURE_FILTER_TRILINEAR);
        if (a->font.texture.id == font.texture.id)
          a->font = font;
      } else if (op == "font-select") {
        size_t identifier = 0;
        in >> identifier;
        done();
        ready();
        if (a->fallback)
          identifier = 0;
        if (identifier >= a->fonts.size())
          throw std::runtime_error("forms font missing");
        a->font = a->fonts[identifier];
      } else if (op == "measure" || op == "measure-spaced") {
        std::string text;
        AbiFloat size = 0;
        in >> text >> size;
        AbiFloat spacing = 0;
        if (op == "measure-spaced")
          in >> spacing;
        done();
        ready();
        Vector2 v = a->get<Vector2 (*)(Font, const char *, float, float)>(
            "MeasureTextEx")(a->font, unhex(text).c_str(), size, spacing);
        char value[100];
        std::snprintf(value, sizeof(value), "%.9g %.9g", double(v.x),
                      double(v.y));
        result = std::string(" ") + value;
      } else if (op == "texture-load") {
        size_t id = 0;
        std::string path;
        int w = 0, h = 0;
        in >> id >> path >> w >> h;
        done();
        ready();
        if (id != a->textures.size())
          throw std::runtime_error("forms texture id");
        auto load = a->get<Image (*)(const char *)>("LoadImage");
        auto unload = a->get<void (*)(Image)>("UnloadImage");
        auto resize = a->get<void (*)(Image *, int, int)>("ImageResize");
        auto upload = a->get<Texture (*)(Image)>("LoadTextureFromImage");
        auto filter = a->get<void (*)(Texture, int)>("SetTextureFilter");
        auto wrap = a->get<void (*)(Texture, int)>("SetTextureWrap");
        Image image = load(unhex(path).c_str());
        if (!image.data || image.width <= 0 || image.height <= 0)
          throw std::runtime_error("forms image load failed");
        if (w != image.width || h != image.height)
          resize(&image, w, h);
        Texture texture = upload(image);
        unload(image);
        a->textures.push_back(texture);
        filter(texture, TEXTURE_FILTER_BILINEAR);
        wrap(texture, TEXTURE_WRAP_CLAMP);
      } else if (op == "texture-rgba") {
        size_t id = 0;
        std::string path;
        in >> id >> path;
        done();
        ready();
        if (id != a->textures.size())
          throw std::runtime_error("texture RGBA id");
        FILE *file = std::fopen(unhex(path).c_str(), "rb");
        if (!file)
          throw std::runtime_error("texture RGBA file");
        std::vector<unsigned char> bytes(128 * 128 * 4);
        bool ok = std::fread(bytes.data(), 1, bytes.size(), file) == bytes.size();
        if (std::fgetc(file) != EOF)
          ok = false;
        std::fclose(file);
        if (!ok)
          throw std::runtime_error("texture RGBA size");
        Image image{bytes.data(), 128, 128, 1,
                    PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
        a->textures.push_back(
            a->get<Texture (*)(Image)>("LoadTextureFromImage")(image));
      } else if (op == "emoji-load") {
        size_t id = 0;
        std::string path;
        in >> id >> path;
        done();
        ready();
        if (id != a->emoji.size())
          throw std::runtime_error("Forms emoji id");
        auto upload = a->get<Texture (*)(Image)>("LoadTextureFromImage");
        FILE *file = std::fopen(unhex(path).c_str(), "rb");
        if (!file)
          throw std::runtime_error("Forms emoji file");
        std::vector<unsigned char> bytes(128 * 128 * 4);
        bool ok =
            std::fread(bytes.data(), 1, bytes.size(), file) == bytes.size();
        if (std::fgetc(file) != EOF)
          ok = false;
        std::fclose(file);
        if (!ok)
          throw std::runtime_error("Forms emoji byte size");
        Image image{bytes.data(), 128, 128, 1,
                    PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
        a->emoji.push_back(upload(image));
      } else if (op == "emoji") {
        size_t id = 0;
        AbiFloat x = 0, y = 0, scale = 0;
        uint32_t rgba = 0;
        in >> id >> x >> y >> scale >> rgba;
        done();
        ready();
        if (id >= a->emoji.size())
          throw std::runtime_error("Forms emoji missing");
        a->get<void (*)(Texture, Vector2, float, float, Color)>(
            "DrawTextureEx")(a->emoji[id], {x, y}, 0, scale, color(rgba));
      } else if (op == "texture-size") {
        size_t id = 0;
        int w = 0, h = 0;
        in >> id >> w >> h;
        done();
        ready();
        if (id >= a->textures.size())
          throw std::runtime_error("forms texture missing");
        a->textures[id].width = w;
        a->textures[id].height = h;
      } else if (op == "texture") {
        size_t id = 0;
        AbiFloat x = 0, y = 0, scale = 0;
        uint32_t c = 0;
        in >> id >> x >> y >> scale >> c;
        done();
        ready();
        if (id >= a->textures.size())
          throw std::runtime_error("forms texture missing");
        a->get<void (*)(Texture, Vector2, float, float, Color)>(
            "DrawTextureEx")(a->textures[id], {x, y}, 0, scale, color(c));
      } else if (op == "texture-v") {
        size_t id = 0;
        AbiFloat x = 0, y = 0;
        uint32_t c = 0;
        in >> id >> x >> y >> c;
        done();
        ready();
        if (id >= a->textures.size())
          throw std::runtime_error("forms texture missing");
        a->get<void (*)(Texture, Vector2, Color)>("DrawTextureV")(
            a->textures[id], {x, y}, color(c));
      } else if (op == "texture-pro") {
        size_t id = 0;
        AbiFloat x = 0, y = 0, w = 0, h = 0;
        uint32_t c = 0;
        in >> id >> x >> y >> w >> h >> c;
        done();
        ready();
        if (id >= a->textures.size())
          throw std::runtime_error("forms texture missing");
        auto t = a->textures[id];
        a->get<void (*)(Texture, Rectangle, Rectangle, Vector2, float, Color)>(
            "DrawTexturePro")(t, {0, 0, float(t.width), float(t.height)},
                              {x, y, w, h}, {0, 0}, 0, color(c));
      } else if (op == "line-ex") {
        AbiFloat x = 0, y = 0, x2 = 0, y2 = 0, w = 0;
        uint32_t c = 0;
        in >> x >> y >> x2 >> y2 >> w >> c;
        done();
        ready();
        a->get<void (*)(Vector2, Vector2, float, Color)>("DrawLineEx")(
            {x, y}, {x2, y2}, w, color(c));
      } else if (op == "shader") {
        std::string vertex, fragment;
        in >> vertex >> fragment;
        done();
        ready();
        a->shader = a->get<Shader (*)(const char *, const char *)>(
            "LoadShaderFromMemory")(unhex(vertex).c_str(),
                                    unhex(fragment).c_str());
        if (!a->shader.id)
          throw std::runtime_error("forms shader failed");
      } else if (op == "shader-begin") {
        done();
        ready();
        a->get<void (*)(Shader)>("BeginShaderMode")(a->shader);
      } else if (op == "shader-end") {
        done();
        a->get<void (*)()>("EndShaderMode")();
      } else if (op == "fps") {
        done();
        ready();
        a->get<void (*)(int, int)>("DrawFPS")(10, 10);
      } else if (op == "strict-fps") {
        done();
        ready();
        if (a->get<int (*)()>("GetFPS")() < 10)
          throw std::runtime_error("Forms FPS dropped critically");
      } else if (op == "target-rgba") {
        std::string path;
        in >> path;
        done();
        ready();
        auto load = a->get<Image (*)(Texture)>("LoadImageFromTexture");
        auto unload = a->get<void (*)(Image)>("UnloadImage");
        Image image = load(a->target.texture);
        FILE *file = std::fopen(unhex(path).c_str(), "wb");
        bool ok =
            file && std::fwrite(image.data, 4,
                                static_cast<size_t>(image.width) *
                                    static_cast<size_t>(image.height),
                                file) == static_cast<size_t>(image.width) *
                                             static_cast<size_t>(image.height);
        if (file) {
          if (std::fclose(file))
            ok = false;
        }
        unload(image);
        if (!ok)
          throw std::runtime_error("Forms recording write failed");
      } else if (op == "target") {
        int w = 0, h = 0;
        in >> w >> h;
        done();
        ready();
        if (a->target.id)
          throw std::runtime_error("duplicate forms target");
        auto filter = a->get<void (*)(Texture, int)>("SetTextureFilter");
        a->target =
            a->get<RenderTexture (*)(int, int)>("LoadRenderTexture")(w, h);
        if (!a->target.id)
          throw std::runtime_error("forms target load failed");
        filter(a->target.texture, TEXTURE_FILTER_BILINEAR);
      } else if (op == "target-begin") {
        done();
        ready();
        if (!a->target.id || a->texture_frame)
          throw std::runtime_error("forms target scope");
        a->get<void (*)(RenderTexture)>("BeginTextureMode")(a->target);
        a->texture_frame = true;
      } else if (op == "target-end") {
        done();
        if (!a->texture_frame || a->matrix)
          throw std::runtime_error("forms target scope");
        a->get<void (*)()>("EndTextureMode")();
        a->texture_frame = false;
      } else if (op == "matrix-push") {
        done();
        ready();
        if (a->matrix)
          throw std::runtime_error("original matrix scope");
        a->get<void (*)()>("rlPushMatrix")();
        a->matrix = true;
      } else if (op == "matrix-scale-xy") {
        AbiFloat x = 0, y = 0;
        in >> x >> y;
        done();
        ready();
        if (!a->matrix)
          throw std::runtime_error("original matrix missing");
        a->get<void (*)(float, float, float)>("rlScalef")(x, y, 1);
      } else if (op == "matrix-scale") {
        AbiFloat scale = 0;
        in >> scale;
        done();
        ready();
        if (a->matrix)
          throw std::runtime_error("forms matrix scope");
        auto push = a->get<void (*)()>("rlPushMatrix");
        auto multiply = a->get<void (*)(float, float, float)>("rlScalef");
        push();
        a->matrix = true;
        multiply(scale, scale, 1);
      } else if (op == "matrix-end" || op == "matrix-pop") {
        done();
        if (!a->matrix)
          throw std::runtime_error("forms matrix missing");
        a->get<void (*)()>("rlPopMatrix")();
        a->matrix = false;
      } else if (op == "target-draw") {
        done();
        ready();
        auto t = a->target.texture;
        a->get<void (*)(Texture, Rectangle, Rectangle, Vector2, float, Color)>(
            "DrawTexturePro")(t, {0, 0, float(t.width), -float(t.height)},
                              {0, 0, float(t.width), float(t.height)}, {0, 0},
                              0, WHITE);
      } else if (op == "export-idle") {
        done();
        ready();
        if (a->frame)
          throw std::runtime_error("duplicate Forms idle export");
        a->get<void (*)()>("BeginDrawing")();
        a->frame = true;
      } else if (op == "begin") {
        done();
        ready();
        if (a->frame)
          throw std::runtime_error("duplicate forms frame");
        a->get<void (*)()>("BeginDrawing")();
        a->frame = true;
      } else if (op == "clear") {
        uint32_t c = 0;
        in >> c;
        done();
        ready();
        a->get<void (*)(Color)>("ClearBackground")(color(c));
      } else if (op == "rect") {
        AbiFloat x = 0, y = 0, w = 0, h = 0;
        uint32_t c = 0;
        in >> x >> y >> w >> h >> c;
        done();
        ready();
        a->get<void (*)(Rectangle, Color)>("DrawRectangleRec")({x, y, w, h},
                                                               color(c));
      } else if (op == "rounded") {
        AbiFloat x = 0, y = 0, w = 0, h = 0, roundness = 0;
        int segments = 0;
        uint32_t c = 0;
        in >> x >> y >> w >> h >> roundness >> segments >> c;
        done();
        ready();
        a->get<void (*)(Rectangle, float, int, Color)>("DrawRectangleRounded")(
            {x, y, w, h}, roundness, segments, color(c));
      } else if (op == "rounded-outline") {
        AbiFloat x = 0, y = 0, w = 0, h = 0, radius = 0, thickness = 0;
        int segments = 0;
        uint32_t c = 0;
        in >> x >> y >> w >> h >> radius >> segments >> thickness >> c;
        done();
        ready();
        a->get<void (*)(Rectangle, float, int, float, Color)>(
            "DrawRectangleRoundedLinesEx")(
            {x, y, w, h}, radius, segments, thickness, color(c));
      } else if (op == "outline-ex") {
        AbiFloat x = 0, y = 0, w = 0, h = 0, thickness = 0;
        uint32_t c = 0;
        in >> x >> y >> w >> h >> thickness >> c;
        done();
        ready();
        a->get<void (*)(Rectangle, float, Color)>("DrawRectangleLinesEx")(
            {x, y, w, h}, thickness, color(c));
      } else if (op == "gradient-h") {
        int x = 0, y = 0, w = 0, h = 0;
        uint32_t c = 0, d = 0;
        in >> x >> y >> w >> h >> c >> d;
        done();
        ready();
        a->get<void (*)(int, int, int, int, Color, Color)>(
            "DrawRectangleGradientH")(x, y, w, h, color(c), color(d));
      } else if (op == "outline") {
        int x = 0, y = 0, w = 0, h = 0;
        uint32_t c = 0;
        in >> x >> y >> w >> h >> c;
        done();
        ready();
        a->get<void (*)(int, int, int, int, Color)>("DrawRectangleLines")(
            x, y, w, h, color(c));
      } else if (op == "circle-gradient") {
        AbiFloat x = 0, y = 0, r = 0;
        uint32_t c = 0, d = 0;
        in >> x >> y >> r >> c >> d;
        done();
        ready();
        a->get<void (*)(Vector2, float, Color, Color)>("DrawCircleGradient")(
            {x, y}, r, color(c), color(d));
      } else if (op == "circle") {
        int x = 0, y = 0;
        AbiFloat r = 0;
        uint32_t c = 0;
        in >> x >> y >> r >> c;
        done();
        ready();
        a->get<void (*)(int, int, float, Color)>("DrawCircle")(x, y, r,
                                                               color(c));
      } else if (op == "line") {
        int x = 0, y = 0, x2 = 0, y2 = 0;
        uint32_t c = 0;
        in >> x >> y >> x2 >> y2 >> c;
        done();
        ready();
        a->get<void (*)(int, int, int, int, Color)>("DrawLine")(x, y, x2, y2,
                                                                color(c));
      } else if (op == "text" || op == "text-spaced") {
        std::string text;
        AbiFloat x = 0, y = 0, size = 0;
        uint32_t c = 0;
        in >> text >> x >> y >> size;
        AbiFloat spacing = 0;
        if (op == "text-spaced")
          in >> spacing;
        in >> c;
        done();
        ready();
        a->get<void (*)(Font, const char *, Vector2, float, float, Color)>(
            "DrawTextEx")(a->font, unhex(text).c_str(), {x, y}, size, spacing,
                          color(c));
      } else if (op == "clip") {
        int x = 0, y = 0, w = 0, h = 0;
        in >> x >> y >> w >> h;
        done();
        ready();
        if (a->clip)
          throw std::runtime_error("duplicate forms clip");
        a->get<void (*)(int, int, int, int)>("BeginScissorMode")(x, y, w, h);
        a->clip = true;
      } else if (op == "unclip") {
        done();
        if (!a->clip)
          throw std::runtime_error("forms clip missing");
        a->get<void (*)()>("EndScissorMode")();
        a->clip = false;
      } else if (op == "end") {
        done();
        if (!a->frame || a->clip)
          throw std::runtime_error("forms frame scope");
        a->get<void (*)()>("EndDrawing")();
        a->frame = false;
      } else if (op == "export" || op == "export-target") {
        std::string path;
        in >> path;
        done();
        ready();
        if (!a->frame && !a->texture_frame)
          throw std::runtime_error("forms capture outside frame");
        auto load = a->get<Image (*)()>("LoadImageFromScreen");
        auto load_target = a->get<Image (*)(Texture)>("LoadImageFromTexture");
        auto flip = a->get<void (*)(Image *)>("ImageFlipVertical");
        auto save = a->get<bool (*)(Image, const char *)>("ExportImage");
        auto unload = a->get<void (*)(Image)>("UnloadImage");
        a->get<void (*)()>("rlDrawRenderBatchActive")();
        Image image =
            op == "export-target" ? load_target(a->target.texture) : load();
        if (op == "export-target")
          flip(&image);
        bool ok = save(image, unhex(path).c_str());
        unload(image);
        if (!ok)
          throw std::runtime_error("forms image export failed");
      } else
        throw std::runtime_error("unknown forms command");
      std::fprintf(reply, "OK%s\n", result.c_str());
    } catch (const std::exception &e) {
      std::fprintf(reply, "ERR %s\n", hex(e.what()).c_str());
    }
    std::fflush(reply);
  }
}

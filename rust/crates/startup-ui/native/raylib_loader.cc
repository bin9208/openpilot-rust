#include "raylib.h"
#include "rlgl.h"
#include <cstdlib>
#include <dlfcn.h>
#include <stdexcept>
#include <string>

namespace {
void *library() {
  static void *handle = [] {
    const char *path = std::getenv("STARTUP_UI_RAYLIB_LIBRARY");
    void *loaded =
        dlopen(path ? path : "libopenpilot-raylib.so", RTLD_NOW | RTLD_LOCAL);
    if (!loaded)
      throw std::runtime_error(std::string("native startup raylib plugin: ") +
                               dlerror());
    using Contract = const char *(*)();
    auto contract = reinterpret_cast<Contract>(
        dlsym(loaded, "openpilot_startup_ui_raylib_contract"));
    if (!contract ||
        std::string(contract()) != "comma-deps-raylib-6.0.0.1.post103") {
      dlclose(loaded);
      throw std::runtime_error(
          "native startup raylib plugin contract mismatch");
    }
    const char *required[] = {"LoadImage",
                              "UnloadImage",
                              "ImageAlphaPremultiply",
                              "ImageResize",
                              "SetConfigFlags",
                              "InitWindow",
                              "IsWindowReady",
                              "CloseWindow",
                              "UnloadTexture",
                              "UnloadFont",
                              "UnloadRenderTexture",
                              "LoadTextureFromImage",
                              "SetTextureFilter",
                              "LoadFont",
                              "LoadFontEx",
                              "GenTextureMipmaps",
                              "MeasureTextEx", "MeasureText",
                              "DrawTextEx",
                              "DrawTexturePro",
                              "DrawRectangleRoundedLinesEx",
                              "DrawRectangleRounded",
                              "BeginScissorMode",
                              "EndScissorMode",
                              "LoadRenderTexture",
                              "BeginTextureMode",
                              "BeginDrawing",
                              "ClearBackground",
                              "rlPushMatrix",
                              "rlScalef",
                              "rlPopMatrix",
                              "EndTextureMode",
                              "EndDrawing",
                              "rlDrawRenderBatchActive",
                              "LoadImageFromTexture",
                              "LoadImageFromScreen",
                              "ImageFlipVertical",
                              "ExportImage",
                              "WindowShouldClose",
                              "SetTargetFPS",
                              "GetFrameTime",
                              "GetTime",
                              "GetTouchPosition",
                              "IsMouseButtonDown",
                              "GetMouseWheelMove",
                              "PollInputEvents",
                              "GetMonitorWidth",
                              "GetMonitorHeight", "ImageFlipHorizontal", "DrawCircleV", "DrawRectangleGradientEx", "DrawLineEx", "DrawCircleGradient", "SetTextureWrap", "LoadShaderFromMemory", "UnloadShader", "BeginShaderMode", "EndShaderMode", "SetWindowTitle", "GetFPS", "DrawFPS", "GetKeyPressed", "GetCharPressed", "IsKeyDown", "IsKeyPressed", "GetMousePosition", "GetShaderLocation", "SetShaderValueV", "SetShaderValue", "SetShaderValueMatrix", "DrawTriangleStrip", "SetTraceLogLevel", "SetTraceLogCallback", "DrawRectangleLines"};
    for (const char *name : required) {
      if (!dlsym(loaded, name)) {
        dlclose(loaded);
        throw std::runtime_error(
            std::string("native startup raylib symbol missing: ") + name);
      }
    }
    // The library remains loaded while any raylib-owned resources or worker
    // calls exist.
    return loaded;
  }();
  return handle;
}
template <typename Function> Function resolve(const char *name) {
  // POSIX dlsym supports converting the symbol address to its declared C
  // function type.
  return reinterpret_cast<Function>(dlsym(library(), name));
}
} // namespace
#define FORWARD(result, name, parameters, arguments)                           \
  result name parameters {                                                     \
    static auto function = resolve<decltype(&name)>(#name);                    \
    return function arguments;                                                 \
  }
FORWARD(Image, LoadImage, (const char *path), (path))
FORWARD(void, UnloadImage, (Image image), (image))
FORWARD(void, ImageAlphaPremultiply, (Image * image), (image))
FORWARD(void, ImageResize, (Image * image, int width, int height),
        (image, width, height))
FORWARD(void, SetConfigFlags, (unsigned int flags), (flags))
FORWARD(void, InitWindow, (int width, int height, const char *title),
        (width, height, title))
FORWARD(bool, IsWindowReady, (), ())
FORWARD(void, CloseWindow, (), ())
FORWARD(void, UnloadTexture, (Texture2D texture), (texture))
FORWARD(void, UnloadFont, (Font font), (font))
FORWARD(void, UnloadRenderTexture, (RenderTexture2D target), (target))
FORWARD(Texture2D, LoadTextureFromImage, (Image image), (image))
FORWARD(void, SetTextureFilter, (Texture2D texture, int filter),
        (texture, filter))
FORWARD(Font, LoadFont, (const char *path), (path))
FORWARD(Font, LoadFontEx,
        (const char *path, int size, const int *points, int count),
        (path, size, points, count))
FORWARD(void, GenTextureMipmaps, (Texture2D * texture), (texture))
FORWARD(int, MeasureText, (const char *text, int size), (text, size))
FORWARD(Vector2, MeasureTextEx,
        (Font font, const char *text, float size, float spacing),
        (font, text, size, spacing))
FORWARD(void, DrawTextEx,
        (Font font, const char *text, Vector2 pos, float size, float spacing,
         Color color),
        (font, text, pos, size, spacing, color))
FORWARD(void, DrawTexturePro,
        (Texture2D texture, Rectangle source, Rectangle dest, Vector2 origin,
         float rotation, Color color),
        (texture, source, dest, origin, rotation, color))
FORWARD(void, DrawRectangleRoundedLinesEx,
        (Rectangle rect, float roundness, int segments, float thick,
         Color color),
        (rect, roundness, segments, thick, color))
FORWARD(void, DrawRectangleRounded,
        (Rectangle rect, float roundness, int segments, Color color),
        (rect, roundness, segments, color))
FORWARD(void, BeginScissorMode, (int x, int y, int width, int height),
        (x, y, width, height))
FORWARD(void, EndScissorMode, (), ())
FORWARD(RenderTexture2D, LoadRenderTexture, (int width, int height),
        (width, height))
FORWARD(void, BeginTextureMode, (RenderTexture2D target), (target))
FORWARD(void, BeginDrawing, (), ())
FORWARD(void, ClearBackground, (Color color), (color))
FORWARD(void, rlPushMatrix, (), ())
FORWARD(void, rlScalef, (float x, float y, float z), (x, y, z))
FORWARD(void, rlPopMatrix, (), ())
FORWARD(void, EndTextureMode, (), ())
FORWARD(void, EndDrawing, (), ())
FORWARD(void, rlDrawRenderBatchActive, (), ())
FORWARD(Image, LoadImageFromTexture, (Texture2D texture), (texture))
FORWARD(Image, LoadImageFromScreen, (), ())
FORWARD(void, ImageFlipVertical, (Image * image), (image))
FORWARD(bool, ExportImage, (Image image, const char *path), (image, path))
FORWARD(bool, WindowShouldClose, (), ())
FORWARD(void, SetTargetFPS, (int fps), (fps))
FORWARD(float, GetFrameTime, (), ())
FORWARD(double, GetTime, (), ())
FORWARD(Vector2, GetTouchPosition, (int slot), (slot))
FORWARD(bool, IsMouseButtonDown, (int slot), (slot))
FORWARD(float, GetMouseWheelMove, (), ())
FORWARD(void, PollInputEvents, (), ())
FORWARD(int, GetMonitorWidth, (int monitor), (monitor))
FORWARD(int, GetMonitorHeight, (int monitor), (monitor))
FORWARD(void, DrawCircleV, (Vector2 center, float radius, Color color), (center,radius,color))
FORWARD(void, DrawRectangleGradientEx, (Rectangle rect, Color top_left, Color bottom_left, Color top_right, Color bottom_right), (rect,top_left,bottom_left,top_right,bottom_right))
FORWARD(void, DrawLineEx, (Vector2 start, Vector2 end, float thick, Color color), (start,end,thick,color))
FORWARD(void, ImageFlipHorizontal, (Image *image), (image))

FORWARD(void, DrawCircleGradient, (Vector2 center, float radius, Color inner, Color outer), (center, radius, inner, outer))

FORWARD(void, SetTextureWrap, (Texture2D texture, int wrap), (texture, wrap))

FORWARD(Shader, LoadShaderFromMemory, (const char *vertex, const char *fragment), (vertex, fragment))
FORWARD(void, UnloadShader, (Shader shader), (shader))
FORWARD(void, BeginShaderMode, (Shader shader), (shader))
FORWARD(void, EndShaderMode, (), ())
FORWARD(void, SetWindowTitle, (const char *title), (title))
FORWARD(int, GetFPS, (), ())
FORWARD(void, DrawFPS, (int x, int y), (x, y))
FORWARD(int, GetKeyPressed, (), ())
FORWARD(int, GetCharPressed, (), ())
FORWARD(bool, IsKeyDown, (int key), (key))
FORWARD(bool, IsKeyPressed, (int key), (key))
FORWARD(Vector2, GetMousePosition, (), ())
FORWARD(int, GetShaderLocation, (Shader shader, const char *name), (shader, name))
FORWARD(void, SetShaderValueV, (Shader shader, int location, const void *values, int kind, int count), (shader, location, values, kind, count))
FORWARD(void, SetShaderValue, (Shader shader, int location, const void *values, int kind), (shader, location, values, kind))
FORWARD(void, SetShaderValueMatrix, (Shader shader, int location, Matrix matrix), (shader, location, matrix))
FORWARD(void, DrawTriangleStrip, (const Vector2 *points, int count, Color color), (points, count, color))
FORWARD(void, SetTraceLogLevel, (int level), (level))
FORWARD(void, SetTraceLogCallback, (TraceLogCallback callback), (callback))
FORWARD(void, DrawRectangleLines, (int x, int y, int width, int height, Color color), (x,y,width,height,color))
#undef FORWARD

#include "raylib.h"
#include "rlgl.h"
#include <cstdlib>
#include <cstdint>
#include <dlfcn.h>
#include <stdexcept>
#include <string>

namespace {
bool library_started = false;
void *library() {
  static void *handle = [] {
    library_started = true;
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
    const char *required[] = {"rlGetVersion", "rlGetActiveFramebuffer", "rlFramebufferComplete", "rlEnableFramebuffer", "rlSetBlendFactorsSeparate","BeginMode3D", "EndMode3D", "rlViewport", "DrawTriangle3D", "DrawLine3D", "DrawSphereEx", "DrawCubeV", "GetWorldToScreenEx", "rlEnableBackfaceCulling", "rlDisableBackfaceCulling", "rlBegin", "rlVertex3f", "rlColor4ub", "rlEnd", "MemAlloc", "MemFree", "UploadMesh", "UnloadMesh", "LoadModelFromMesh", "IsModelValid", "UnloadModel", "DrawModelEx", "BeginBlendMode", "rlSetBlendFactors", "EndBlendMode", "GenImageColor", "ImageDrawTextEx", "LoadImage", "LoadImageFromMemory", "GetFontDefault", "DrawRectangleRec", "DrawTriangle", "rlTranslatef", "ImageFormat", "DrawRing", "DrawCircleLines", "UpdateTexture", "SetShaderValueTexture",
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
                              "DrawTextEx", "DrawText", "DrawLine",
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
                              "SetExitKey",
                              "IsMouseButtonPressed",
                              "GetFrameTime",
                              "GetTime",
                              "GetTouchPosition",
                              "IsMouseButtonDown",
                              "GetMouseWheelMove",
                              "PollInputEvents",
                              "GetMonitorWidth",
                              "GetMonitorHeight", "ImageFlipHorizontal", "DrawCircleV", "DrawRectangleGradientEx", "DrawLineEx", "DrawCircleGradient", "SetTextureWrap", "LoadShaderFromMemory", "UnloadShader", "BeginShaderMode", "EndShaderMode", "SetWindowTitle", "GetFPS", "DrawFPS", "GetKeyPressed", "GetCharPressed", "IsKeyDown", "IsKeyPressed", "GetMousePosition", "GetShaderLocation", "SetShaderValueV", "SetShaderValue", "SetShaderValueMatrix", "DrawSplineLinear", "SetTraceLogLevel", "SetTraceLogCallback", "DrawRectangleLines"};
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
void openpilot_install_hud_gbm_cleanup(const char *path, int owns_fd) {
  static void *cleanup = nullptr;
  if (cleanup) return;
  if (library_started) throw std::runtime_error("HUD GBM cleanup must load before raylib");
  void *loaded = dlopen(path, RTLD_NOW | RTLD_GLOBAL);
  if (!loaded) throw std::runtime_error(std::string("HUD GBM cleanup: ") + dlerror());
  using Enable = int (*)(int);
  auto enable = reinterpret_cast<Enable>(dlsym(loaded,"openpilot_hud_gbm_cleanup_enable"));
  if (!enable || !enable(owns_fd)) {dlclose(loaded);throw std::runtime_error("HUD GBM cleanup initialization failed");}
  // Keep the interposed functions resident across every HUD retry.
  cleanup = loaded;
}
bool openpilot_raylib_symbol_available(const char *name) {return dlsym(library(),name)!=nullptr;}
// Raylib's ES2 rlGetActiveFramebuffer is a zero-returning stub. Resolve the
// actual binding reader once, before accepting the window, and query the GL
// context that owns the caller's rotated/recording target on each draw.
uint32_t openpilot_active_framebuffer() {
  using GetInteger = void (*)(unsigned int, int *);
  static GetInteger es_reader = []() -> GetInteger {
    const auto version = resolve<decltype(&rlGetVersion)>("rlGetVersion")();
    if (version != RL_OPENGL_ES_20 && version != RL_OPENGL_ES_30) return nullptr;
    void *gles = dlopen("libGLESv2.so", RTLD_NOW | RTLD_LOCAL);
    if (!gles) gles = dlopen("libGLESv2.so.2", RTLD_NOW | RTLD_LOCAL);
    if (!gles) throw std::runtime_error(std::string("overlay GLES library: ")+dlerror());
    auto query = reinterpret_cast<GetInteger>(dlsym(gles, "glGetIntegerv"));
    if (!query) {dlclose(gles);throw std::runtime_error("overlay glGetIntegerv unavailable");}
    // As with the raylib provider, keep this loader handle for its full lifetime.
    return query;
  }();
  if (!es_reader) return rlGetActiveFramebuffer();
  int binding = 0;
  es_reader(0x8CA6, &binding); // GL_FRAMEBUFFER_BINDING, valid in ES2.
  return static_cast<uint32_t>(binding);
}

#define FORWARD(result, name, parameters, arguments)                           \
  result name parameters {                                                     \
    static auto function = resolve<decltype(&name)>(#name);                    \
    return function arguments;                                                 \
  }
FORWARD(Image, LoadImage, (const char *path), (path))
FORWARD(Image, LoadImageFromMemory, (const char *extension, const unsigned char *bytes, int size), (extension,bytes,size))
FORWARD(Font, GetFontDefault, (), ())
FORWARD(void, DrawRectangleRec, (Rectangle rect, Color color), (rect,color))
FORWARD(void, DrawTriangle, (Vector2 a, Vector2 b, Vector2 c, Color color), (a,b,c,color))
FORWARD(void, rlTranslatef, (float x,float y,float z), (x,y,z))
FORWARD(Image, GenImageColor, (int width,int height,Color color), (width,height,color))
FORWARD(void, ImageDrawTextEx, (Image *image,Font font,const char *text,Vector2 position,float size,float spacing,Color color), (image,font,text,position,size,spacing,color))
FORWARD(void, BeginMode3D, (Camera3D camera), (camera))
FORWARD(void, EndMode3D, (), ())
FORWARD(void, rlViewport, (int x,int y,int width,int height), (x,y,width,height))
FORWARD(void, DrawTriangle3D, (Vector3 a,Vector3 b,Vector3 c,Color color), (a,b,c,color))
FORWARD(void, DrawLine3D, (Vector3 a,Vector3 b,Color color), (a,b,color))
FORWARD(void, DrawSphereEx, (Vector3 center,float radius,int rings,int slices,Color color), (center,radius,rings,slices,color))
FORWARD(void, DrawCubeV, (Vector3 center,Vector3 size,Color color), (center,size,color))
FORWARD(void, DrawTriangleStrip3D, (const Vector3 *points,int count,Color color), (points,count,color))
FORWARD(Vector2, GetWorldToScreenEx, (Vector3 point,Camera camera,int width,int height), (point,camera,width,height))
FORWARD(void, rlEnableBackfaceCulling, (), ())
FORWARD(void, rlDisableBackfaceCulling, (), ())
FORWARD(void, rlBegin, (int mode), (mode))
FORWARD(void, rlVertex3f, (float x,float y,float z), (x,y,z))
FORWARD(void, rlColor4ub, (unsigned char r,unsigned char g,unsigned char b,unsigned char a), (r,g,b,a))
FORWARD(void, rlEnd, (), ())
FORWARD(void *, MemAlloc, (unsigned int size), (size))
FORWARD(void, MemFree, (void *value), (value))
FORWARD(void, UploadMesh, (Mesh *mesh,bool dynamic), (mesh,dynamic))
FORWARD(void, UnloadMesh, (Mesh mesh), (mesh))
FORWARD(Model, LoadModelFromMesh, (Mesh mesh), (mesh))
FORWARD(bool, IsModelValid, (Model model), (model))
FORWARD(void, UnloadModel, (Model model), (model))
FORWARD(void, DrawModelEx, (Model model,Vector3 position,Vector3 axis,float angle,Vector3 scale,Color color), (model,position,axis,angle,scale,color))
FORWARD(void, BeginBlendMode, (int mode), (mode))
FORWARD(void, rlSetBlendFactors, (int source,int destination,int equation), (source,destination,equation))
FORWARD(void, EndBlendMode, (), ())
FORWARD(void, UpdateTexture, (Texture2D texture, const void *pixels), (texture, pixels))
FORWARD(void, SetShaderValueTexture, (Shader shader, int location, Texture2D texture), (shader, location, texture))
FORWARD(void, ImageFormat, (Image *image, int format), (image, format))
FORWARD(void, DrawRing, (Vector2 center, float inner, float outer, float start, float end, int segments, Color color), (center, inner, outer, start, end, segments, color))
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
FORWARD(void, DrawText, (const char *text, int x, int y, int size, Color color), (text,x,y,size,color))
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
FORWARD(unsigned char *, ExportImageToMemory, (Image image,const char *extension,int *size), (image,extension,size))
FORWARD(bool, WindowShouldClose, (), ())
FORWARD(void, SetTargetFPS, (int fps), (fps))
FORWARD(void, SetExitKey, (int key), (key))
FORWARD(bool, IsMouseButtonPressed, (int button), (button))
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
FORWARD(void, DrawLine, (int x1, int y1, int x2, int y2, Color color), (x1,y1,x2,y2,color))
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
FORWARD(void, DrawRectangleLinesEx, (Rectangle rect, float thickness, Color color), (rect,thickness,color))
FORWARD(void, DrawSplineLinear, (const Vector2 *points, int count, float thick, Color color), (points, count, thick, color))
FORWARD(void, DrawCircleLines, (int x, int y, float radius, Color color), (x, y, radius, color))

FORWARD(unsigned int, rlGetActiveFramebuffer, (), ())
FORWARD(bool, rlFramebufferComplete, (unsigned int id), (id))
FORWARD(void, rlEnableFramebuffer, (unsigned int id), (id))
FORWARD(void, rlSetBlendFactorsSeparate, (int srcRGB,int dstRGB,int srcAlpha,int dstAlpha,int eqRGB,int eqAlpha), (srcRGB,dstRGB,srcAlpha,dstAlpha,eqRGB,eqAlpha))

#undef FORWARD

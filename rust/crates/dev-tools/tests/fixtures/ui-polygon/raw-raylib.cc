#include "raylib.h"
extern "C" {
void SetConfigFlags(unsigned int) {}
void InitWindow(int, int, const char *) {}
void SetTargetFPS(int) {}
void BeginDrawing() {}
void ClearBackground(Color) {}
Shader LoadShaderFromMemory(const char *, const char *) {
  return Shader{1, nullptr};
}
int GetShaderLocation(Shader, const char *) { return 0; }
void SetShaderValue(Shader, int, const void *, int) {}
void SetShaderValueV(Shader, int, const void *, int, int) {}
void SetShaderValueMatrix(Shader, int, Matrix) {}
void BeginShaderMode(Shader) {}
void DrawTriangleStrip(const Vector2 *, int, Color) {}
void EndShaderMode() {}
void rlDrawRenderBatchActive() {}
Image LoadImageFromScreen() {
  static unsigned char pixel[] = {0, 0, 0, 255};
  return Image{pixel, 1, 1, 1, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
}
bool ExportImage(Image, const char *) { return true; }
void UnloadImage(Image) {}
void EndDrawing() {}
void UnloadShader(Shader) {}
void CloseWindow() {}
}

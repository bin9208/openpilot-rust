#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include "rlgl.h"
#include <mutex>
#include <clocale>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string>
#if defined(__GLIBC__)
#include <malloc.h>
#endif
bool openpilot_raylib_symbol_available(const char *name);
uint32_t openpilot_active_framebuffer();
void openpilot_install_hud_gbm_cleanup(const char *,int);
namespace startup_ui {
void hud_trim_heap() {
#if defined(__GLIBC__)
  malloc_trim(0);
#endif
}
void hud_graphics_cleanup(rust::Str path, bool owns_fd) {
  ::openpilot_install_hud_gbm_cleanup(std::string(path).c_str(),owns_fd ? 1 : 0);
}
bool try_process_locale(rust::Str candidate) {
  const std::string name(candidate);
  if(name.find('\0')!=std::string::npos)throw std::runtime_error("locale contains NUL");
  return std::setlocale(LC_ALL,name.c_str())!=nullptr;
}
static void log_callback(int level,const char *format,va_list arguments) noexcept {
  try {
    va_list copy;
    va_copy(copy,arguments);
    int count=vsnprintf(nullptr,0,format,copy);
    va_end(copy);
    if (count<0) {
      trace_log(level,{reinterpret_cast<const uint8_t *>(format),std::strlen(format)});
      return;
    }
    std::vector<char> text(size_t(count)+1);
    vsnprintf(text.data(),text.size(),format,arguments);
    trace_log(level,{reinterpret_cast<const uint8_t *>(text.data()),size_t(count)});
  } catch (...) {
    // External C callbacks must not propagate C++ exceptions through raylib.
    std::fputs("raylib log formatting failed\n",stderr);
  }
}
static std::mutex window_mutex;
static bool window_owned = false;
static ::Color color(uint32_t value) {
  return {uint8_t(value), uint8_t(value >> 8), uint8_t(value >> 16),
          uint8_t(value >> 24)};
}
static ::Rectangle rectangle(Rect value) {
  return {value.x, value.y, value.width, value.height};
}
Image::Image(const char *path) : value(LoadImage(path)) {
  if (!value.data)
    throw std::runtime_error("raylib image load failed");
}
Image::Image(rust::Str extension, rust::Slice<const uint8_t> bytes) {
  if (bytes.empty() || bytes.size() > size_t(std::numeric_limits<int>::max()))
    throw std::runtime_error("invalid encoded image size");
  value = LoadImageFromMemory(std::string(extension).c_str(), bytes.data(), int(bytes.size()));
  if (!value.data) throw std::runtime_error("raylib encoded image load failed");
}
Image::Image(int32_t width, int32_t height, uint32_t tint) {
  if (width <= 0 || height <= 0) throw std::runtime_error("blank image dimensions must be positive");
  value = GenImageColor(width,height,color(tint));
  if (!value.data) throw std::runtime_error("raylib image allocation failed");
}
Image::~Image() { UnloadImage(value); }
Image::Image(::Texture2D texture) : value(LoadImageFromTexture(texture)) {
  if(!value.data)throw std::runtime_error("texture readback failed");
}
int32_t Image::width() const { return value.width; }
int32_t Image::height() const { return value.height; }
int32_t Image::format() const {return value.format;}
void Image::rgba_format() {ImageFormat(&value,PIXELFORMAT_UNCOMPRESSED_R8G8B8A8);}
void Image::flip_vertical() {ImageFlipVertical(&value);}
rust::Slice<const uint8_t> Image::rgba_view() const {
  if(!value.data || value.width<=0 || value.height<=0 || value.format!=PIXELFORMAT_UNCOMPRESSED_R8G8B8A8)throw std::runtime_error("image is not RGBA");
  const uint64_t size=uint64_t(value.width)*uint64_t(value.height)*4;
  if(size>uint64_t(std::numeric_limits<std::ptrdiff_t>::max()))throw std::runtime_error("image RGBA size overflow");
  return {static_cast<const uint8_t *>(value.data),size_t(size)};
}
rust::Vec<uint8_t> Image::encode(rust::Str extension) const {
  int size=0;auto *data=ExportImageToMemory(value,std::string(extension).c_str(),&size);
  struct Guard {unsigned char *value;~Guard(){MemFree(value);}} guard{data};
  if(!data || size<=0)throw std::runtime_error("image memory encode failed");
  rust::Vec<uint8_t> encoded;encoded.reserve(size);
  for(int i=0;i<size;++i)encoded.push_back(data[i]);
  return encoded;
}
void Image::export_file(rust::Str path) const {if(!ExportImage(value,std::string(path).c_str()))throw std::runtime_error("image export failed");}
rust::Vec<uint8_t> Image::rgba() {
  ImageFormat(&value, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8);
  if (!value.data || value.width <= 0 || value.height <= 0 || value.format != PIXELFORMAT_UNCOMPRESSED_R8G8B8A8)
    throw std::runtime_error("image RGBA conversion failed");
  const uint64_t size = uint64_t(value.width) * uint64_t(value.height) * 4;
  if (size > SIZE_MAX) throw std::runtime_error("image RGBA size overflow");
  rust::Vec<uint8_t> result;
  result.reserve(size_t(size));
  const auto *data = static_cast<const uint8_t *>(value.data);
  for (size_t index = 0; index < size_t(size); ++index) result.push_back(data[index]);
  return result;
}
void Image::premultiply() { ImageAlphaPremultiply(&value); }
void Image::flip_horizontal() { ImageFlipHorizontal(&value); }
void Image::resize(int32_t width, int32_t height) {
  if (width > 0 && height > 0)
    ImageResize(&value, width, height);
}
Surface::Surface(int32_t width, int32_t height, rust::Str title,
                 uint32_t flags, int32_t level, bool apply_flags) {
  std::lock_guard<std::mutex> lock(window_mutex);
  if (window_owned)
    throw std::runtime_error("raylib window already owned");
  SetTraceLogLevel(level);
  SetTraceLogCallback(log_callback);
  if(apply_flags)SetConfigFlags(flags);
  InitWindow(width, height, std::string(title).c_str());
  if (!IsWindowReady())
    throw std::runtime_error("raylib window initialization failed");
  try {openpilot_active_framebuffer();} catch (...) {CloseWindow();throw;}
  window_owned = true;
}
Surface::~Surface() {
  std::lock_guard<std::mutex> lock(window_mutex);
  for (auto &model : models) if (model.meshCount > 0) UnloadModel(model);
  for (auto &mesh : meshes) if (mesh.vertexCount > 0) UnloadMesh(mesh);
  for (auto &value : targets) if (value.id) UnloadRenderTexture(value);
  for (auto &texture : textures)
    if (texture.id) UnloadTexture(texture);
  for (size_t index = 0; index < fonts.size(); ++index)
    if (int64_t(index) != default_font_index) UnloadFont(fonts[index]);
  for (auto &shader : shaders)
    if (shader.id) UnloadShader(shader);
  if (target.id)
    UnloadRenderTexture(target);
  if (shader.id)
    UnloadShader(shader);
  if (IsWindowReady())
    CloseWindow();
  window_owned = false;
}
uint32_t Surface::texture(Image &image, int32_t logical_width,
                          int32_t logical_height) {
  auto value = LoadTextureFromImage(image.value);
  SetTextureFilter(value, TEXTURE_FILTER_BILINEAR);
  SetTextureWrap(value, TEXTURE_WRAP_CLAMP);
  if (logical_width > 0 && logical_height > 0) {
    value.width = logical_width;
    value.height = logical_height;
  }
  textures.push_back(value);
  return textures.size() - 1;
}
void Surface::clear(uint32_t tint) noexcept { ClearBackground(color(tint)); }
uint32_t Surface::default_font() {
  if (default_font_index < 0) {
    default_font_index = int64_t(fonts.size());
    fonts.push_back(GetFontDefault());
  }
  return uint32_t(default_font_index);
}
void Surface::font_bilinear(uint32_t index) {
  SetTextureFilter(fonts.at(index).texture, TEXTURE_FILTER_BILINEAR);
}
void Surface::rectangle_rec(Rect rect, uint32_t tint) {
  DrawRectangleRec(rectangle(rect), color(tint));
}
void Surface::rectangle_lines_ex(Rect rect, float thickness, uint32_t tint) {
  DrawRectangleLinesEx(rectangle(rect), thickness, color(tint));
}
void Surface::triangle(Point a, Point b, Point c, uint32_t tint) {
  DrawTriangle({a.x,a.y}, {b.x,b.y}, {c.x,c.y}, color(tint));
}
void Surface::push_matrix() { rlPushMatrix(); }
void Surface::pop_matrix() { rlPopMatrix(); }
void Surface::scale(float x,float y,float z) { rlScalef(x,y,z); }
void Surface::translate(float x,float y,float z) { rlTranslatef(x,y,z); }
void Surface::begin_capture() {
  if (!target.id) throw std::runtime_error("capture render target missing");
  BeginTextureMode(target);
}
void Surface::end_capture() { EndTextureMode(); }
TargetInfo Surface::capture_info() const {if(!target.id)throw std::runtime_error("capture target missing");return {target.id,target.texture.id,target.texture.width,target.texture.height};}
void Surface::capture_draw(TexturePaint paint) {if(!target.id)throw std::runtime_error("capture target missing");DrawTexturePro(target.texture,rectangle(paint.source),rectangle(paint.destination),{paint.origin.x,paint.origin.y},paint.rotation,color(paint.tint));}
void Surface::frame_begin() {BeginDrawing();}
void Surface::capture_filter(int32_t filter) {if(!target.id)throw std::runtime_error("capture target missing");SetTextureFilter(target.texture,filter);}
std::unique_ptr<Image> Surface::capture_image() const {if(!target.id)throw std::runtime_error("capture target missing");rlDrawRenderBatchActive();return std::make_unique<Image>(target.texture);}
std::unique_ptr<Image> Surface::target_image(uint32_t index) const {const auto value=targets.at(index);if(!value.id)throw std::runtime_error("render target released");rlDrawRenderBatchActive();return std::make_unique<Image>(value.texture);}
void Surface::text_batch(rust::Str text,rust::Slice<const TextPaint> paints) {
  const auto encoded=std::string(text);
  for(const auto &paint:paints) DrawTextEx(fonts.at(paint.font),encoded.c_str(),{paint.position.x,paint.position.y},paint.size,paint.spacing,color(paint.color));
}
void Surface::frame_end() {EndDrawing();}
void Surface::image_text(Image &image, rust::Str text, TextPaint paint) {
  ImageDrawTextEx(&image.value, fonts.at(paint.font), std::string(text).c_str(), {paint.position.x,paint.position.y}, paint.size, paint.spacing, color(paint.color));
}
void Surface::texture_filter(uint32_t index, int32_t filter) {
  const auto value = textures.at(index);
  if (!value.id) throw std::runtime_error("texture has been released");
  SetTextureFilter(value, filter);
}
void Surface::texture_size(uint32_t index,int32_t width,int32_t height) {auto &value=textures.at(index);if(!value.id || width<=0 || height<=0)throw std::runtime_error("invalid logical texture dimensions");value.width=width;value.height=height;}
void Surface::uniform_texture(uint32_t shader_index,rust::Str name,uint32_t texture_index) {const auto shader=shaders.at(shader_index);const auto texture=textures.at(texture_index);if(!shader.id || !texture.id)throw std::runtime_error("shader texture released");SetShaderValueTexture(shader,GetShaderLocation(shader,std::string(name).c_str()),texture);}
static ::Vector3 vector3(Vector3 value) { return {value.x,value.y,value.z}; }
static ::Camera3D camera3d(Camera3D value) { return {vector3(value.position),vector3(value.target),vector3(value.up),value.fovy,value.projection}; }
Points3D::Points3D(rust::Slice<const float> input) {
  if (input.size() % 3 || input.size()/3 > size_t(std::numeric_limits<int>::max())) throw std::runtime_error("invalid 3D point buffer");
  values.reserve(input.size()/3);
  for (size_t index=0;index<input.size();index+=3) values.push_back({input[index],input[index+1],input[index+2]});
}
Points2D::Points2D(uint32_t capacity) {
  if(capacity>uint32_t(std::numeric_limits<int>::max()))throw std::runtime_error("2D point capacity exceeds native range");
  values.resize(capacity);
}
void Points2D::update(rust::Slice<const Point> points) {
  if(points.size()>values.size())throw std::runtime_error("2D point update exceeds retained capacity");
  for(size_t index=0;index<points.size();++index)values[index]={points[index].x,points[index].y};
  length=points.size();
}
std::unique_ptr<Points2D> points2d(uint32_t capacity) {return std::make_unique<Points2D>(capacity);}
void Surface::strip2d(const Points2D &points,uint32_t offset,uint32_t count,uint32_t tint) {
  if(uint64_t(offset)+count>points.length || count>uint32_t(std::numeric_limits<int>::max()))throw std::runtime_error("2D point draw exceeds uploaded range");
  if(count>=3) {if(!strip2d_available())throw std::runtime_error("2D triangle strip primitive is unavailable");DrawTriangleStrip(points.values.data()+offset,int(count),color(tint));}
}
bool Surface::strip2d_available() const {return openpilot_raylib_symbol_available("DrawTriangleStrip");}
void Surface::mode3d_begin(Camera3D camera) { BeginMode3D(camera3d(camera)); }
void Surface::mode3d_end() { EndMode3D(); }
void Surface::viewport(int32_t x,int32_t y,int32_t width,int32_t height) { rlViewport(x,y,width,height); }
void Surface::triangle3d(Vector3 a,Vector3 b,Vector3 c,uint32_t tint) { DrawTriangle3D(vector3(a),vector3(b),vector3(c),color(tint)); }
void Surface::line3d(Vector3 a,Vector3 b,uint32_t tint) { DrawLine3D(vector3(a),vector3(b),color(tint)); }
void Surface::sphere(Vector3 center,float radius,int32_t rings,int32_t slices,uint32_t tint) { DrawSphereEx(vector3(center),radius,rings,slices,color(tint)); }
void Surface::cube_v(Vector3 center,Vector3 size,uint32_t tint) { DrawCubeV(vector3(center),vector3(size),color(tint)); }
bool Surface::strip_available() const {return openpilot_raylib_symbol_available("DrawTriangleStrip3D");}
void Surface::strip3d(const Points3D &points,uint32_t tint) {if(!strip_available())throw std::runtime_error("triangle strip primitive unavailable");DrawTriangleStrip3D(points.values.data(),int(points.values.size()),color(tint));}
Point Surface::world_to_screen(Vector3 point,Camera3D camera,int32_t width,int32_t height) const { auto value=GetWorldToScreenEx(vector3(point),camera3d(camera),width,height); return {value.x,value.y}; }
void Surface::culling(bool enabled) { if (enabled) rlEnableBackfaceCulling(); else rlDisableBackfaceCulling(); }
void Surface::vertices_begin(int32_t mode) { rlBegin(mode); }
void Surface::vertex3(Vector3 point) { rlVertex3f(point.x,point.y,point.z); }
void Surface::color4(uint32_t tint) { auto value=color(tint); rlColor4ub(value.r,value.g,value.b,value.a); }
void Surface::vertices_end() { rlEnd(); }
uint32_t Surface::target_create(int32_t width,int32_t height) {
  if (width<=0 || height<=0) throw std::runtime_error("render target dimensions must be positive");
  auto value=LoadRenderTexture(width,height);
  if (!value.id) throw std::runtime_error("render target allocation failed");
  for (size_t index=0;index<targets.size();++index) if (!targets[index].id) {targets[index]=value;return index;}
  targets.push_back(value);return targets.size()-1;
}
uint32_t Surface::overlay_create(int32_t width,int32_t height) {
  if(width<=0 || height<=0)throw std::runtime_error("overlay dimensions must be positive");
  auto value=LoadRenderTexture(width,height);
  // Preserve partial/zero allocation metadata so the Rust owner can fall back
  // once and release every allocated attachment, as the source does.
  for(size_t index=0;index<targets.size();++index)if(!targets[index].id && !targets[index].texture.id && !targets[index].depth.id){targets[index]=value;return index;}
  targets.push_back(value);return targets.size()-1;
}
uint32_t Surface::framebuffer() const {return openpilot_active_framebuffer();}
bool Surface::overlay_complete(uint32_t index) const {
  const auto value=targets.at(index);
  return value.id && value.texture.id && rlFramebufferComplete(value.id);
}
void Surface::framebuffer_bind(uint32_t framebuffer) {rlEnableFramebuffer(framebuffer);}
void Surface::render_flush() {rlDrawRenderBatchActive();}
void Surface::coverage_begin() {
  ClearBackground(BLANK);
  rlSetBlendFactorsSeparate(0x0302,0x0303,1,0x0303,0x8006,0x8006);
  BeginBlendMode(BLEND_CUSTOM_SEPARATE);
}
void Surface::overlay_composite(uint32_t index,float width,float height) {
  const auto value=targets.at(index);
  if(!value.id || !value.texture.id)throw std::runtime_error("overlay target unavailable");
  BeginBlendMode(BLEND_ALPHA_PREMULTIPLY);
  DrawTexturePro(value.texture,{0,0,float(value.texture.width),-float(value.texture.height)},{0,0,width,height},{0,0},0,WHITE);
  EndBlendMode();
}
void Surface::target_release(uint32_t index) { auto &value=targets.at(index); if (value.id || value.texture.id || value.depth.id) UnloadRenderTexture(value); value={}; }
void Surface::target_begin(uint32_t index) { const auto value=targets.at(index); if (!value.id) throw std::runtime_error("render target released"); BeginTextureMode(value); }
void Surface::external_target_begin(TargetInfo info) {
  if(!info.framebuffer || !info.texture || info.width<=0 || info.height<=0)throw std::runtime_error("invalid external render target");
  RenderTexture2D value{};value.id=info.framebuffer;value.texture={info.texture,info.width,info.height,1,PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
  BeginTextureMode(value);
}
void Surface::target_end() { EndTextureMode(); }
TargetInfo Surface::target_info(uint32_t index) const { const auto value=targets.at(index);if(!value.id)throw std::runtime_error("render target released");return {value.id,value.texture.id,value.texture.width,value.texture.height}; }
rust::Vec<uint8_t> Surface::target_pixels(uint32_t index,bool flip_vertical) const {
  const auto value=targets.at(index);if(!value.id)throw std::runtime_error("render target released");
  rlDrawRenderBatchActive(); auto image=LoadImageFromTexture(value.texture);
  if(!image.data)throw std::runtime_error("target readback failed");
  struct Guard {::Image &value;~Guard(){UnloadImage(value);}} guard{image};
  if(flip_vertical)ImageFlipVertical(&image);
  if(image.format!=PIXELFORMAT_UNCOMPRESSED_R8G8B8A8)ImageFormat(&image,PIXELFORMAT_UNCOMPRESSED_R8G8B8A8);
  const auto length=uint64_t(image.width)*uint64_t(image.height)*4;
  rust::Vec<uint8_t> bytes;bytes.reserve(length);const auto *data=static_cast<const uint8_t *>(image.data);
  for(uint64_t i=0;i<length;++i) bytes.push_back(data[i]);
  return bytes;
}
void Surface::target_draw(uint32_t index,TexturePaint paint) {const auto value=targets.at(index);if(!value.id)throw std::runtime_error("render target released");DrawTexturePro(value.texture,rectangle(paint.source),rectangle(paint.destination),{paint.origin.x,paint.origin.y},paint.rotation,color(paint.tint));}
void Surface::target_filter(uint32_t index,int32_t filter) { const auto value=targets.at(index);if(!value.id)throw std::runtime_error("render target released");SetTextureFilter(value.texture,filter); }
uint32_t Surface::mesh_create(rust::Slice<const float> vertices,rust::Slice<const float> normals,rust::Slice<const uint8_t> colors) {
  if(vertices.size()<9 || vertices.size()%9 || vertices.size()!=normals.size() || colors.size()!=vertices.size()/3*4 || vertices.size()>size_t(std::numeric_limits<unsigned int>::max()/sizeof(float)))throw std::runtime_error("invalid mesh buffers");
  ::Mesh mesh{};mesh.vertexCount=int(vertices.size()/3);mesh.triangleCount=mesh.vertexCount/3;
  mesh.vertices=static_cast<float *>(MemAlloc(vertices.size()*sizeof(float)));mesh.normals=static_cast<float *>(MemAlloc(normals.size()*sizeof(float)));mesh.colors=static_cast<unsigned char *>(MemAlloc(colors.size()));
  if(!mesh.vertices || !mesh.normals || !mesh.colors){MemFree(mesh.vertices);MemFree(mesh.normals);MemFree(mesh.colors);throw std::runtime_error("mesh allocation failed");}
  std::memcpy(mesh.vertices,vertices.data(),vertices.size()*sizeof(float));std::memcpy(mesh.normals,normals.data(),normals.size()*sizeof(float));std::memcpy(mesh.colors,colors.data(),colors.size());
  struct Guard {::Mesh *value;~Guard(){if(value)UnloadMesh(*value);}} guard{&mesh};
  for(size_t index=0;index<meshes.size();++index)if(meshes[index].vertexCount==0){meshes[index]=mesh;guard.value=nullptr;return index;}
  meshes.push_back(mesh);guard.value=nullptr;return meshes.size()-1;
}
void Surface::mesh_upload(uint32_t index,bool dynamic) {
  auto &value=meshes.at(index);if(value.vertexCount<=0)throw std::runtime_error("mesh released");
  if(value.vboId)throw std::runtime_error("mesh already uploaded");
  UploadMesh(&value,dynamic);
}
void Surface::mesh_release(uint32_t index) {auto &value=meshes.at(index);if(value.vertexCount>0){UnloadMesh(value);value={};}}
uint32_t Surface::model_from_mesh(uint32_t mesh_index) {
  auto &mesh=meshes.at(mesh_index);if(mesh.vertexCount<=0 || !mesh.vboId)throw std::runtime_error("mesh not uploaded");
  auto value=LoadModelFromMesh(mesh);mesh={};
  if(!IsModelValid(value)){UnloadModel(value);throw std::runtime_error("model allocation failed");}
  struct Guard {::Model *value;~Guard(){if(value)UnloadModel(*value);}} guard{&value};
  for(size_t index=0;index<models.size();++index)if(models[index].meshCount==0){models[index]=value;guard.value=nullptr;return index;}
  models.push_back(value);guard.value=nullptr;return models.size()-1;
}
void Surface::model_release(uint32_t index) {auto &value=models.at(index);if(value.meshCount>0){UnloadModel(value);value={};}}
void Surface::model_draw(uint32_t index,ModelPaint paint) {auto value=models.at(index);if(value.meshCount==0)throw std::runtime_error("model released");DrawModelEx(value,vector3(paint.position),vector3(paint.rotation_axis),paint.rotation_angle,vector3(paint.scale),color(paint.tint));}
void Surface::shader_begin(uint32_t index) {auto value=shaders.at(index);if(!value.id)throw std::runtime_error("shader released");BeginShaderMode(value);}
void Surface::shader_end() {EndShaderMode();}
void Surface::blend_custom(int32_t source,int32_t destination,int32_t equation) {rlSetBlendFactors(source,destination,equation);BeginBlendMode(BLEND_CUSTOM);}
void Surface::blend_end() {EndBlendMode();}
void Surface::texture_release(uint32_t index) noexcept {
  if (index < textures.size() && textures[index].id) {
    UnloadTexture(textures[index]);
    textures[index] = {};
  }
}
uint32_t Surface::pixel_texture(int32_t width, int32_t height, rust::Slice<const uint8_t> rgba) {
  if (width <= 0 || height <= 0 || uint64_t(width)*uint64_t(height)*4 != rgba.size())
    throw std::runtime_error("invalid RGBA texture dimensions");
  ::Image image{const_cast<uint8_t *>(rgba.data()), width, height, 1, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
  auto value=LoadTextureFromImage(image);
  if (!value.id) throw std::runtime_error("RGBA texture upload failed");
  for (size_t index=0;index<textures.size();++index) if (!textures[index].id) {textures[index]=value;return index;}
  textures.push_back(value);
  return textures.size()-1;
}
void Surface::smooth_texture(uint32_t texture) {
  const auto value = textures.at(texture);
  SetTextureFilter(value, TEXTURE_FILTER_BILINEAR);
  SetTextureWrap(value, TEXTURE_WRAP_CLAMP);
}
void Surface::pixel_update(uint32_t index,rust::Slice<const uint8_t> rgba) {
  const auto value=textures.at(index);
  if(!value.id || value.format!=PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 || uint64_t(value.width)*uint64_t(value.height)*4!=rgba.size()) throw std::runtime_error("invalid RGBA texture update");
  UpdateTexture(value,rgba.data());
}
void Surface::ring(Ring ring) {
  DrawRing({ring.center.x, ring.center.y}, ring.inner, ring.outer, ring.start, ring.end, ring.segments, color(ring.color));
}
uint32_t Surface::font(rust::Str path, int32_t size,
                       rust::Slice<const int32_t> points, bool atlas,
                       bool mipmaps) {
  std::vector<int> owned(points.begin(), points.end());
  auto value = atlas ? LoadFont(std::string(path).c_str())
                     : LoadFontEx(std::string(path).c_str(), size, owned.data(),
                                  owned.size());
  if (mipmaps && value.texture.id) {
    GenTextureMipmaps(&value.texture);
    SetTextureFilter(value.texture, TEXTURE_FILTER_TRILINEAR);
  }
  fonts.push_back(value);
  return fonts.size() - 1;
}
Point Surface::measure(uint32_t font, rust::Str text, float size,
                       float spacing) const {
  auto value =
      MeasureTextEx(fonts.at(font), std::string(text).c_str(), size, spacing);
  return {value.x, value.y};
}
void Surface::text(uint32_t font, rust::Str text, Point position, float size,
                   float spacing, uint32_t tint) {
  DrawTextEx(fonts.at(font), std::string(text).c_str(),
             {position.x, position.y}, size, spacing, color(tint));
}
void Surface::draw_texture(uint32_t texture, Rect rect, Point origin,
                           float rotation) {
  auto value = textures.at(texture);
  DrawTexturePro(value, {0, 0, float(value.width), float(value.height)},
                 rectangle(rect), {origin.x, origin.y}, rotation, WHITE);
}
void Surface::tinted_texture(uint32_t texture, Rect source, Rect destination, Point origin, float rotation, uint32_t tint) {
  DrawTexturePro(textures.at(texture), rectangle(source), rectangle(destination), {origin.x,origin.y}, rotation, color(tint));
}
void Surface::circle(Point center, float radius, uint32_t tint) { DrawCircleV({center.x,center.y},radius,color(tint)); }
void Surface::circle_lines(int32_t x, int32_t y, float radius, uint32_t tint) { DrawCircleLines(x,y,radius,color(tint)); }
void Surface::circle_gradient(Point center, float radius, uint32_t inner, uint32_t outer) { DrawCircleGradient({center.x,center.y},radius,color(inner),color(outer)); }
void Surface::gradient(Rect rect, uint32_t top_left, uint32_t bottom_left, uint32_t top_right, uint32_t bottom_right) {
  DrawRectangleGradientEx(rectangle(rect), color(top_left), color(bottom_left), color(top_right), color(bottom_right));
}
void Surface::line(Point start, Point end, float thick, uint32_t tint) { DrawLineEx({start.x,start.y},{end.x,end.y},thick,color(tint)); }
void Surface::integer_line(int32_t x1, int32_t y1, int32_t x2, int32_t y2, uint32_t tint) { DrawLine(x1,y1,x2,y2,color(tint)); }
void Surface::default_text(rust::Str text, int32_t x, int32_t y, int32_t size, uint32_t tint) {
  const std::string owned(text);
  DrawText(owned.c_str(),x,y,size,color(tint));
}
void Surface::rounded_segments(Rect rect, float roundness, int32_t segments, uint32_t tint, bool border) {
  if (border) DrawRectangleRoundedLinesEx(rectangle(rect), roundness, segments, 2, color(tint));
  else DrawRectangleRounded(rectangle(rect), roundness, segments, color(tint));
}
int32_t Surface::measure_default(rust::Str text, int32_t size) const {
  const std::string owned(text);
  return MeasureText(owned.c_str(), size);
}
void Surface::rounded_outline(Rect rect, float roundness, int32_t segments, float thickness, uint32_t tint) {
  DrawRectangleRoundedLinesEx(rectangle(rect), roundness, segments, thickness, color(tint));
}
void Surface::rounded(Rect rect, float roundness, uint32_t tint, bool border) {
  if (border)
    DrawRectangleRoundedLinesEx(rectangle(rect), roundness, 10, 2, color(tint));
  else
    DrawRectangleRounded(rectangle(rect), roundness, 10, color(tint));
}
void Surface::scissor(Rect rect, bool enabled) {
  if (enabled)
    BeginScissorMode(int(rect.x), int(rect.y), int(rect.width),
                     int(rect.height));
  else
    EndScissorMode();
}
void Surface::render_target(int32_t width, int32_t height) {
  if (target.id) return;
  const auto previous=openpilot_active_framebuffer();
  rlDrawRenderBatchActive();
  target = LoadRenderTexture(width, height);
  rlEnableFramebuffer(previous);
  if (!target.id)
    throw std::runtime_error("raylib render texture failed");
  SetTextureFilter(target.texture, TEXTURE_FILTER_BILINEAR);
}
void Surface::begin(float scale) {
  frame_target=target;
  frame_open=true;
  if (frame_target.id)
    BeginTextureMode(frame_target);
  else
    BeginDrawing();
  ClearBackground(BLACK);
  if (scale != 1) {
    rlPushMatrix();
    rlScalef(scale, scale, 1);
  }
}
void Surface::finish_content(float scale) {
  if (scale != 1)
    rlPopMatrix();
  if (frame_target.id) {
    EndTextureMode();
    BeginDrawing();
    ClearBackground(BLACK);
    if (shader.id) BeginShaderMode(shader);
    DrawTexturePro(
        frame_target.texture,
        {0, 0, float(frame_target.texture.width), -float(frame_target.texture.height)},
        {0, 0, float(frame_target.texture.width), float(frame_target.texture.height)},
        {0, 0}, 0, WHITE);
    if (shader.id) EndShaderMode();
  }
}
void Surface::present() { EndDrawing(); frame_open=false; }
void Surface::end(float scale) { finish_content(scale); present(); }
bool Surface::has_render_target() const { return target.id != 0; }
bool Surface::has_capture_frame() const {return frame_target.id != 0;}
void Surface::release_render_target() {
  if(frame_open)throw std::runtime_error("render target release during a frame");
  if(target.id)UnloadRenderTexture(target);
  target={};frame_target={};
}
void Surface::burn_in(rust::Str vertex, rust::Str fragment) {
  if (shader.id) return;
  shader = LoadShaderFromMemory(std::string(vertex).c_str(), std::string(fragment).c_str());
  if (!shader.id) throw std::runtime_error("burn-in shader failed");
}
static rust::Vec<uint8_t> read_texture_pixels(::Texture texture) {
  rlDrawRenderBatchActive();
  auto image = LoadImageFromTexture(texture);
  if (!image.data) throw std::runtime_error("recording texture readback failed");
  struct ImageGuard { ::Image value; ~ImageGuard() { UnloadImage(value); } } guard{image};
  if (image.format != PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 || image.width <= 0 || image.height <= 0)
    throw std::runtime_error("recording image is not RGBA");
  auto count=uint64_t(image.width)*uint64_t(image.height)*4;
  rust::Vec<uint8_t> bytes;
  bytes.reserve(count);
  const auto *pixels=static_cast<const uint8_t *>(image.data);
  for (uint64_t i=0; i<count; ++i) bytes.push_back(pixels[i]);
  return bytes;
}
rust::Vec<uint8_t> Surface::capture_pixels() const {
  if (!target.id) throw std::runtime_error("capture render target missing");
  return read_texture_pixels(target.texture);
}
rust::Vec<uint8_t> Surface::capture_frame_pixels() const {
  if (!frame_target.id) throw std::runtime_error("recording frame target missing");
  return read_texture_pixels(frame_target.texture);
}
void Surface::set_title(rust::Str title) { SetWindowTitle(std::string(title).c_str()); }
int32_t Surface::fps() const { return GetFPS(); }
void Surface::draw_fps(int32_t x, int32_t y) { DrawFPS(x,y); }
int32_t Surface::key_pressed() const { return GetKeyPressed(); }
int32_t Surface::char_pressed() const { return GetCharPressed(); }
bool Surface::key_down(int32_t key) const { return IsKeyDown(key); }
bool Surface::key_started(int32_t key) const { return IsKeyPressed(key); }
void Surface::exit_key(int32_t key) { SetExitKey(key); }
bool Surface::mouse_pressed(int32_t button) const { return IsMouseButtonPressed(button); }
Point Surface::mouse_position() const { auto position=GetMousePosition();return {position.x,position.y}; }

void Surface::screenshot(rust::Str path) const {
  rlDrawRenderBatchActive();
  auto image =
      target.id ? LoadImageFromTexture(target.texture) : LoadImageFromScreen();
  if (target.id)
    ImageFlipVertical(&image);
  bool saved = ExportImage(image, std::string(path).c_str());
  UnloadImage(image);
  if (!saved)
    throw std::runtime_error("screenshot export failed");
}
void Surface::screen_screenshot(rust::Str path) const {
  rlDrawRenderBatchActive();auto image=LoadImageFromScreen();bool saved=ExportImage(image,std::string(path).c_str());UnloadImage(image);if(!saved)throw std::runtime_error("screen export failed");
}
void Surface::rectangle_lines(int32_t x,int32_t y,int32_t width,int32_t height,uint32_t tint){DrawRectangleLines(x,y,width,height,color(tint));}
bool Surface::should_close() const { return WindowShouldClose(); }
void Surface::target_fps(int32_t fps) { SetTargetFPS(fps); }
float Surface::frame_time() const { return GetFrameTime(); }
double Surface::time() const { return GetTime(); }
Sample Surface::sample(int32_t slot) const {
  auto point = GetTouchPosition(slot);
  return {point.x, point.y, IsMouseButtonDown(slot)};
}
float Surface::wheel() const { return GetMouseWheelMove(); }
void poll_input() { PollInputEvents(); }
Sample sample_input(int32_t slot) {
  auto point = GetTouchPosition(slot);
  return {point.x, point.y, IsMouseButtonDown(slot)};
}
std::unique_ptr<Surface> create(int32_t width, int32_t height, rust::Str title,
                                uint32_t flags) {
  return std::make_unique<Surface>(width, height, title, flags);
}
std::unique_ptr<Surface> create_with_trace(int32_t width,int32_t height,rust::Str title,uint32_t flags,int32_t level) {return std::make_unique<Surface>(width,height,title,flags,level);}
std::unique_ptr<Surface> create_inheriting_flags(int32_t width,int32_t height,rust::Str title,int32_t level) {return std::make_unique<Surface>(width,height,title,0,level,false);}
void trace_level(int32_t level) {SetTraceLogLevel(level);}
Point monitor() {
  std::lock_guard<std::mutex> lock(window_mutex);
  if (window_owned)
    throw std::runtime_error("raylib window already owned");
  InitWindow(1, 1, "");
  Point point{float(GetMonitorWidth(0)), float(GetMonitorHeight(0))};
  CloseWindow();
  return point;
}
std::unique_ptr<Image> image(rust::Str path) {
  return std::make_unique<Image>(std::string(path).c_str());
}
std::unique_ptr<Image> image_encoded(rust::Str extension, rust::Slice<const uint8_t> bytes) {
  return std::make_unique<Image>(extension, bytes);
}
std::unique_ptr<Image> image_blank(int32_t width, int32_t height, uint32_t tint) {
  return std::make_unique<Image>(width,height,tint);
}
std::unique_ptr<Points3D> points3d(rust::Slice<const float> values) {return std::make_unique<Points3D>(values);}
void export_rgba(int32_t width, int32_t height, rust::Slice<const uint8_t> bytes, rust::Str path) {
  if (width <= 0 || height <= 0 || uint64_t(width) * uint64_t(height) * 4 != bytes.size()) throw std::runtime_error("RGBA export dimensions mismatch");
  ::Image image{const_cast<uint8_t *>(bytes.data()),width,height,1,PIXELFORMAT_UNCOMPRESSED_R8G8B8A8};
  if (!ExportImage(image, std::string(path).c_str())) throw std::runtime_error("RGBA export failed");
}
} // namespace startup_ui

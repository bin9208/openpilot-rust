// Independent public C API reference; no CXX bridge or private SDL structs.
#include <cstdint>
#include <cstdio>
#include <dlfcn.h>
#include <filesystem>
#include <fstream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

class Library {
  void* handle;
public:
  explicit Library(const char* path) : handle(dlopen(path, RTLD_NOW | RTLD_LOCAL)) {
    if (!handle) throw std::runtime_error(dlerror());
  }
  ~Library() { dlclose(handle); }
  Library(const Library&) = delete;
  Library& operator=(const Library&) = delete;
  template<class T> T symbol(const char* name) {
    void* address = dlsym(handle, name);
    if (!address) throw std::runtime_error(std::string("missing font provider symbol: ") + name);
    return reinterpret_cast<T>(address);
  }
};
struct Rect { int x, y, w, h; };
struct Color { std::uint8_t r, g, b, a; };

int main(int argc, char** argv) try {
  if (argc != 5) throw std::runtime_error("SDL TTF FONT OUTPUT_DIRECTORY required");
  const std::filesystem::path output(argv[4]);
  if (!std::filesystem::create_directory(output)) throw std::runtime_error("reference output already exists");
  Library sdl(argv[1]), ttf(argv[2]);
  const auto quit_sdl = sdl.symbol<void(*)(std::uint32_t)>("SDL_QuitSubSystem");
  if (sdl.symbol<int(*)(std::uint32_t)>("SDL_InitSubSystem")(0x20)) throw std::runtime_error("reference video init failed");
  const auto quit_ttf = ttf.symbol<void(*)()>("TTF_Quit");
  if (ttf.symbol<int(*)()>("TTF_Init")()) throw std::runtime_error("reference TTF init failed");
  const auto close_font = ttf.symbol<void(*)(void*)>("TTF_CloseFont");
  const auto free_surface = sdl.symbol<void(*)(void*)>("SDL_FreeSurface");
  const auto destroy_renderer = sdl.symbol<void(*)(void*)>("SDL_DestroyRenderer");
  const auto destroy_window = sdl.symbol<void(*)(void*)>("SDL_DestroyWindow");
  auto write_image = [&](void* surface, const std::string& name) {
    if (!surface) throw std::runtime_error("missing reference surface");
    Rect bounds{};
    sdl.symbol<void(*)(void*, Rect*)>("SDL_GetClipRect")(surface, &bounds);
    if (bounds.w <= 0 || bounds.h <= 0 || bounds.w > 4096 || bounds.h > 4096) throw std::runtime_error("reference surface bounds invalid");
    const std::unique_ptr<void, decltype(destroy_renderer)> renderer(
        sdl.symbol<void*(*)(void*)>("SDL_CreateSoftwareRenderer")(surface), destroy_renderer);
    if (!renderer) throw std::runtime_error("reference software renderer failed");
    std::vector<std::uint8_t> pixels(static_cast<std::size_t>(bounds.w)*bounds.h*4);
    if (sdl.symbol<int(*)(void*, const Rect*, std::uint32_t, void*, int)>("SDL_RenderReadPixels")(
        renderer.get(), nullptr, 0x16762004, pixels.data(), bounds.w*4)) throw std::runtime_error("reference pixel read failed");
    std::ofstream stream(output/name, std::ios::binary);
    stream.write(reinterpret_cast<const char*>(pixels.data()), static_cast<std::streamsize>(pixels.size()));
    stream.close();
    if (!stream) throw std::runtime_error("reference pixel output failed");
    return bounds;
  };
  std::ofstream dimensions(output/"dimensions.txt");
  for (int index = 0; index < 4; ++index) {
    const int size = index < 2 ? 12 : 18;
    const bool bold = index%2;
    const char* text = bold ? "Å β [literal]" : "native replay 63/35";
    const std::unique_ptr<void, decltype(close_font)> font(ttf.symbol<void*(*)(const char*, int)>("TTF_OpenFont")(argv[3], size), close_font);
    if (!font) throw std::runtime_error("reference font open failed");
    ttf.symbol<void(*)(void*, int)>("TTF_SetFontStyle")(font.get(), bold ? 1 : 0);
    const std::unique_ptr<void, decltype(free_surface)> glyph(ttf.symbol<void*(*)(void*, const char*, Color)>("TTF_RenderUTF8_Blended")(
        font.get(), text, Color{230,233,240,255}), free_surface);
    const auto bounds = write_image(glyph.get(), std::to_string(index)+".glyph.rgba");
    dimensions << bounds.w << ' ' << bounds.h << '\n';
    const std::unique_ptr<void, decltype(destroy_window)> window(sdl.symbol<void*(*)(const char*, int, int, int, int, std::uint32_t)>("SDL_CreateWindow")(
        "Owned native font", 0, 0, bounds.w, bounds.h, 4), destroy_window);
    if (!window) throw std::runtime_error("reference window creation failed");
    void* target = sdl.symbol<void*(*)(void*)>("SDL_GetWindowSurface")(window.get());
    // Use the independently rendered glyph's converted RGBA pixels, as the Rust
    // public present() API does. No private SDL surface layout is assumed.
    std::ifstream glyph_pixels(output/(std::to_string(index)+".glyph.rgba"), std::ios::binary);
    std::vector<std::uint8_t> pixels(static_cast<std::size_t>(bounds.w)*bounds.h*4);
    glyph_pixels.read(reinterpret_cast<char*>(pixels.data()), static_cast<std::streamsize>(pixels.size()));
    if (!glyph_pixels) throw std::runtime_error("reference glyph input failed");
    const std::unique_ptr<void, decltype(free_surface)> input(sdl.symbol<void*(*)(void*, int, int, int, int, std::uint32_t)>("SDL_CreateRGBSurfaceWithFormatFrom")(
        pixels.data(), bounds.w, bounds.h, 32, bounds.w*4, 0x16762004), free_surface);
    if (!input || !target || sdl.symbol<int(*)(void*, int)>("SDL_SetSurfaceBlendMode")(input.get(), 0)
        || sdl.symbol<int(*)(void*, const Rect*, void*, Rect*)>("SDL_UpperBlit")(input.get(), nullptr, target, nullptr)
        || sdl.symbol<int(*)(void*)>("SDL_UpdateWindowSurface")(window.get())) throw std::runtime_error("reference window present failed");
    write_image(target, std::to_string(index)+".window.rgba");
  }
  dimensions.close();
  if (!dimensions) throw std::runtime_error("reference dimensions output failed");
  std::ifstream maps("/proc/self/maps");
  std::ofstream mapped(output/"provider-maps.txt");
  mapped << maps.rdbuf();
  mapped.close();
  if (!mapped) throw std::runtime_error("reference maps output failed");
  quit_ttf(); quit_sdl(0x20);
  std::puts("4 direct C glyph/surface/software-window cycles passed");
  return 0;
} catch (const std::exception& error) {
  std::fprintf(stderr, "%s\n", error.what());
  return 1;
}

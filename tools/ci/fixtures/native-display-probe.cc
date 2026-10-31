// Independent C ABI reference for the original display providers; no Python ABI.
#include <cstdint>
#include <cstdio>
#include <dlfcn.h>
#include <fstream>
#include <iterator>
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
    if (!address) throw std::runtime_error(std::string("missing provider symbol: ") + name);
    return reinterpret_cast<T>(address);
  }
};

int main(int argc, char** argv) try {
  if (argc != 10) throw std::runtime_error("SDL TTF TURBO INPUT OUTPUT WIDTH HEIGHT QUALITY MAPS required");
  Library sdl(argv[1]), ttf(argv[2]), turbo(argv[3]);
  struct Version { std::uint8_t major, minor, patch; };
  Version version{};
  sdl.symbol<void(*)(Version*)>("SDL_GetVersion")(&version);
  if (version.major != 2 || version.minor != 28 || version.patch != 4) throw std::runtime_error("SDL version differs");
  const auto* ttf_version = ttf.symbol<const Version*(*)()>("TTF_Linked_Version")();
  if (!ttf_version || ttf_version->major != 2 || ttf_version->minor != 20 || ttf_version->patch != 1)
    throw std::runtime_error("SDL_ttf version differs");
  if (ttf.symbol<int(*)()>("TTF_Init")() != 0) throw std::runtime_error("SDL_ttf initialization failed");
  if (ttf.symbol<int(*)()>("TTF_WasInit")() != 1) throw std::runtime_error("SDL_ttf state differs");
  ttf.symbol<void(*)()>("TTF_Quit")();
  if (ttf.symbol<int(*)()>("TTF_WasInit")() != 0) throw std::runtime_error("SDL_ttf cleanup failed");
  const int width = std::stoi(argv[6]), height = std::stoi(argv[7]), quality = std::stoi(argv[8]);
  if (width <= 0 || height <= 0 || width > 65500 || height > 65500 || quality < 1 || quality > 100)
    throw std::runtime_error("invalid reference dimensions/quality");
  std::ifstream input(argv[4], std::ios::binary);
  if (!input) throw std::runtime_error("missing packed RGBA input");
  const std::vector<unsigned char> pixels((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  if (pixels.size() != static_cast<std::size_t>(width) * height * 4) throw std::runtime_error("reference pixel extent differs");
  const auto destroy = turbo.symbol<int(*)(void*)>("tjDestroy");
  const std::unique_ptr<void, decltype(destroy)> compressor(turbo.symbol<void*(*)()>("tjInitCompress")(), destroy);
  if (!compressor) throw std::runtime_error("reference compressor creation failed");
  unsigned char* encoded = nullptr;
  unsigned long bytes = 0;
  const int status = turbo.symbol<int(*)(void*, const unsigned char*, int, int, int, int,
      unsigned char**, unsigned long*, int, int, int)>("tjCompress2")(
      compressor.get(), pixels.data(), width, 0, height, 7, &encoded, &bytes, 2, quality, 0);
  const auto release = turbo.symbol<void(*)(unsigned char*)>("tjFree");
  const std::unique_ptr<unsigned char, decltype(release)> owned(encoded, release);
  if (status || !encoded || bytes < 4 || bytes > 128*1024*1024 || encoded[0] != 0xff || encoded[1] != 0xd8)
    throw std::runtime_error("reference TurboJPEG encoding failed");
  std::ofstream output(argv[5], std::ios::binary);
  output.write(reinterpret_cast<const char*>(encoded), static_cast<std::streamsize>(bytes));
  output.close();
  if (!output) throw std::runtime_error("reference JPEG output failed");
  std::ifstream maps("/proc/self/maps");
  std::ofstream mapped(argv[9]);
  mapped << maps.rdbuf();
  mapped.close();
  if (!mapped) throw std::runtime_error("reference provider maps failed");
  std::puts("{\"sdl_version\":\"2.28.4\",\"ttf_version\":\"2.20.1\",\"ttf_init_cleanup\":true,\"turbojpeg_encoded\":true}");
  return 0;
} catch (const std::exception& error) {
  std::fprintf(stderr, "%s\n", error.what());
  return 1;
}

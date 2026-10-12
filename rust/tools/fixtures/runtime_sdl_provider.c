#include <SDL.h>
#include <SDL_ttf.h>
#include <dlfcn.h>
#include <stdio.h>

#define LOAD(handle, type, name) type name = (type)dlsym(handle, #name); if (!name) { fprintf(stderr, "missing %s: %s\n", #name, dlerror()); return 2; }

typedef int (*init_fn)(Uint32);
typedef void (*quit_fn)(void);
typedef SDL_Window *(*window_fn)(const char *, int, int, int, int, Uint32);
typedef void (*destroy_fn)(SDL_Window *);
typedef int (*ttf_init_fn)(void);
typedef TTF_Font *(*font_fn)(const char *, int);
typedef void (*close_font_fn)(TTF_Font *);
typedef int (*size_fn)(TTF_Font *, const char *, int *, int *);
typedef SDL_Surface *(*text_fn)(TTF_Font *, const char *, SDL_Color);
typedef void (*free_fn)(SDL_Surface *);
typedef SDL_Surface *(*convert_fn)(SDL_Surface *, Uint32, Uint32);
typedef const SDL_version *(*version_fn)(void);

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  void *sdl = dlopen(argv[1], RTLD_NOW | RTLD_GLOBAL);
  void *ttf = dlopen(argv[2], RTLD_NOW | RTLD_GLOBAL);
  if (!sdl || !ttf) { fprintf(stderr, "provider load: %s\n", dlerror()); return 2; }
  LOAD(sdl, init_fn, SDL_Init)
  LOAD(sdl, quit_fn, SDL_Quit)
  LOAD(sdl, window_fn, SDL_CreateWindow)
  LOAD(sdl, destroy_fn, SDL_DestroyWindow)
  LOAD(sdl, free_fn, SDL_FreeSurface)
  LOAD(sdl, convert_fn, SDL_ConvertSurfaceFormat)
  LOAD(ttf, ttf_init_fn, TTF_Init)
  LOAD(ttf, quit_fn, TTF_Quit)
  LOAD(ttf, font_fn, TTF_OpenFont)
  LOAD(ttf, close_font_fn, TTF_CloseFont)
  LOAD(ttf, size_fn, TTF_SizeUTF8)
  LOAD(ttf, text_fn, TTF_RenderUTF8_Blended)
  LOAD(ttf, version_fn, TTF_Linked_Version)
  if (SDL_Init(SDL_INIT_VIDEO) != 0 || TTF_Init() != 0) return 3;
  SDL_Window *window = SDL_CreateWindow("Owned provider validation", 0, 0, 96, 64, SDL_WINDOW_HIDDEN);
  TTF_Font *font = TTF_OpenFont(argv[3], 18);
  if (!window || !font) return 4;
  const char *text = "Speed 42 km/h";
  int width = 0, height = 0;
  if (TTF_SizeUTF8(font, text, &width, &height) != 0) return 5;
  SDL_Color color = {255, 255, 255, 255};
  SDL_Surface *surface = TTF_RenderUTF8_Blended(font, text, color);
  if (!surface) return 6;
  const SDL_version *version = TTF_Linked_Version();
  fprintf(stderr, "ttf=%u.%u.%u width=%d height=%d surface=%dx%d pitch=%d\n", version->major, version->minor, version->patch,
          width, height, surface->w, surface->h, surface->pitch);
  SDL_Surface *rgba = SDL_ConvertSurfaceFormat(surface, SDL_PIXELFORMAT_RGBA32, 0);
  if (!rgba) return 7;
  const size_t row_bytes = (size_t)rgba->w * 4;
  for (int row = 0; row < rgba->h; ++row) {
    const unsigned char *pixels = (const unsigned char *)rgba->pixels + (size_t)row * (size_t)rgba->pitch;
    if (fwrite(pixels, 1, row_bytes, stdout) != row_bytes) return 8;
  }
  SDL_FreeSurface(rgba);
  SDL_FreeSurface(surface);
  TTF_CloseFont(font);
  SDL_DestroyWindow(window);
  TTF_Quit();
  SDL_Quit();
  dlclose(ttf);
  dlclose(sdl);
  return 0;
}

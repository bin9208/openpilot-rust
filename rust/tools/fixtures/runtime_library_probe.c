#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
  if (argc < 3) return 2;
  void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  if (!library) { fprintf(stderr, "%s\n", dlerror()); return 3; }
  int first = 3;
  if (strcmp(argv[2], "contract") == 0) {
    if (argc < 4) return 2;
    const char *(*contract)(void) = (const char *(*)(void))dlsym(library, "openpilot_startup_ui_raylib_contract");
    if (!contract || strcmp(contract(), argv[3]) != 0) return 4;
    first = 4;
  }
  void *(*extension)(const char *) = NULL;
  if (strcmp(argv[2], "extensions") == 0) {
    extension = (void *(*)(const char *))dlsym(library, "eglGetProcAddress");
    if (!extension) return 5;
  }
  for (int i = first; i < argc; ++i) {
    void *address = extension ? extension(argv[i]) : dlsym(library, argv[i]);
    if (!address) { fprintf(stderr, "missing symbol: %s\n", argv[i]); return 6; }
  }
  printf("{\"resolved\":%d}\n", argc - first);
  dlclose(library);
  return 0;
}

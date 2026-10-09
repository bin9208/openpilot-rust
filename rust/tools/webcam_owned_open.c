// Owned Linux entrypoint fixture only; never linked into the webcam runtime.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char *owned_path(const char *path) {
  if (!path) return path;
  const char *keys[] = {"ROAD", "WIDE", "DRIVER"};
  for (size_t i = 0; i < 3; ++i) {
    char alias_key[64], file_key[64];
    snprintf(alias_key, sizeof(alias_key), "WEBCAM_OWNED_%s_ALIAS", keys[i]);
    snprintf(file_key, sizeof(file_key), "WEBCAM_OWNED_%s_FILE", keys[i]);
    const char *alias = getenv(alias_key), *file = getenv(file_key);
    const char *root = getenv("WEBCAM_OWNED_ROOT");
    if (alias && file && root && strncmp(alias, "/dev/videowebcam255_", 20) == 0 &&
        strcmp(alias, path) == 0 && strncmp(file, root, strlen(root)) == 0 &&
        file[strlen(root)] == '/') {
      dprintf(STDERR_FILENO, "OWNED_OPEN %s => %s\n", path, file);
      return file;
    }
  }
  return path;
}

typedef int (*open_fn)(const char *, int, ...);

int open(const char *path, int flags, ...) {
  open_fn actual = (open_fn)dlsym(RTLD_NEXT, "open");
  if (!actual) _exit(121);
  mode_t mode = 0;
  if ((flags & O_CREAT) || (flags & O_TMPFILE) == O_TMPFILE) {
    va_list ap; va_start(ap, flags); mode = va_arg(ap, unsigned int); va_end(ap);
  }
  return actual(owned_path(path), flags, mode);
}

int open64(const char *path, int flags, ...) {
  open_fn actual = (open_fn)dlsym(RTLD_NEXT, "open64");
  if (!actual) _exit(122);
  mode_t mode = 0;
  if ((flags & O_CREAT) || (flags & O_TMPFILE) == O_TMPFILE) {
    va_list ap; va_start(ap, flags); mode = va_arg(ap, unsigned int); va_end(ap);
  }
  return actual(owned_path(path), flags, mode);
}

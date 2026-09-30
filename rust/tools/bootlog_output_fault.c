// Controlled host oracle: fail only boot artifact output, preserving real Params I/O.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static FILE *fault_stream;
static const char *redirect(const char *path) {
  const char *mode = getenv("BOOTLOG_OUTPUT_FAULT");
  if (!mode || !strstr(path, "/boot/") || !strstr(path, ".zst")) return path;
  if (!strcmp(mode, "open")) { errno = EACCES; return NULL; }
  return !strcmp(mode, "full") ? "/dev/full" : path;
}
int open64(const char *path, int flags, ...) {
  mode_t mode = 0;
  if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); }
  const char *actual = redirect(path);
  if (!actual) return -1;
  int (*next)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
  return next(actual, flags, mode);
}
FILE *fopen(const char *path, const char *mode) {
  const char *actual = redirect(path);
  if (!actual) return NULL;
  FILE *(*next)(const char *, const char *) = dlsym(RTLD_NEXT, "fopen");
  FILE *stream = next(actual, mode);
  const char *fault = getenv("BOOTLOG_OUTPUT_FAULT");
  if (fault && !strcmp(fault, "close") && strstr(path, "/boot/") && strstr(path, ".zst")) fault_stream = stream;
  return stream;
}
int fclose(FILE *stream) {
  int fail = stream == fault_stream;
  if (fail) fault_stream = NULL;
  int (*next)(FILE *) = dlsym(RTLD_NEXT, "fclose");
  int result = next(stream);
  if (fail) { errno = EIO; return EOF; }
  return result;
}

// Host oracle diagnostics replace only the external cloud log transport.
#include <cstdarg>
#include <cstdio>

void cloudlog_e(int, const char *, int, const char *, const char *format, ...) {
  va_list args;
  va_start(args, format);
  vfprintf(stderr, format, args);
  fputc('\n', stderr);
  va_end(args);
}

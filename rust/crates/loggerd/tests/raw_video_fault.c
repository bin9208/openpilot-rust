// Host-only faults at real stdio boundaries, limited to the raw road stream.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static FILE *raw_stream;
struct faulty_file { int fd; int interrupt; };
static struct faulty_file *raw_cookie;
static unsigned int raw_writes;

static ssize_t cookie_write(void *opaque, const char *bytes, size_t length) {
  struct faulty_file *file = opaque;
  if (file->interrupt) {
    file->interrupt = 0;
    errno = EINTR;
    return -1;
  }
  return write(file->fd, bytes, length);
}

static int cookie_close(void *opaque) {
  struct faulty_file *file = opaque;
  int result = close(file->fd);
  raw_cookie = NULL;
  free(file);
  return result;
}

FILE *fopen(const char *path, const char *mode) {
  FILE *(*next)(const char *, const char *) = dlsym(RTLD_NEXT, "fopen");
  const char *fault = getenv("LOGGERD_RAW_FAULT");
  const char suffix[] = "/fcamera.hevc";
  size_t length = strlen(path);
  int selected = fault && length >= sizeof(suffix) - 1 &&
      !strcmp(path + length - (sizeof(suffix) - 1), suffix);
  FILE *stream;
  if (selected && !strcmp(fault, "eintr")) {
    raw_cookie = calloc(1, sizeof(*raw_cookie));
    assert(raw_cookie);
    raw_cookie->fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0666);
    assert(raw_cookie->fd >= 0);
    cookie_io_functions_t functions = {.write = cookie_write, .close = cookie_close};
    stream = fopencookie(raw_cookie, mode, functions);
    assert(stream);
  } else {
    stream = next(path, mode);
  }
  if (selected) raw_stream = stream;
  return stream;
}

size_t fwrite(const void *bytes, size_t size, size_t count, FILE *stream) {
  size_t (*next)(const void *, size_t, size_t, FILE *) = dlsym(RTLD_NEXT, "fwrite");
  if (raw_cookie && stream == raw_stream && ++raw_writes == 2) {
    // Fail the real FILE's buffered header flush. libc owns its sticky ferror
    // state and discards those header bytes. The following *actual* fwrite can
    // accept its full payload although the previously lost header is unrecoverable.
    raw_cookie->interrupt = 1;
    assert(fflush(stream) == EOF && ferror(stream) && errno == EINTR);
    size_t written = next(bytes, size, count, stream);
    assert(written == count && ferror(stream));
    const char marker[] = "loggerd-io-fault: full-count ferror EINTR\n";
    (void)write(STDERR_FILENO, marker, sizeof(marker) - 1);
    errno = EINTR;
    return written;
  }
  return next(bytes, size, count, stream);
}

int fclose(FILE *stream) {
  const char *fault = getenv("LOGGERD_RAW_FAULT");
  int selected = stream && stream == raw_stream;
  int fail = selected && fault && !strcmp(fault, "close");
  if (selected) raw_stream = NULL;
  int (*next)(FILE *) = dlsym(RTLD_NEXT, "fclose");
  int result = next(stream);
  if (fail) {
    const char marker[] = "loggerd-io-fault: fclose ENOSPC\n";
    (void)write(STDERR_FILENO, marker, sizeof(marker) - 1);
    errno = ENOSPC;
    return EOF;
  }
  return result;
}

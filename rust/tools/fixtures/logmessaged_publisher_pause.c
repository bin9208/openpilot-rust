#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int ftruncate(int fd, off_t length) {
  int (*original)(int, off_t) = dlsym(RTLD_NEXT, "ftruncate");
  int result = original(fd, length);
  const char *gate = getenv("LOG_QA_PUBLISHER_GATE");
  char descriptor[64], path[4096];
  snprintf(descriptor, sizeof(descriptor), "/proc/self/fd/%d", fd);
  ssize_t count = readlink(descriptor, path, sizeof(path) - 1);
  if (result == 0 && gate && count > 0) {
    path[count] = '\0';
    if (strstr(path, "/errorLogMessage")) {
      for (int i = 0; i < 10000 && access(gate, F_OK) == 0; ++i) usleep(1000);
    }
  }
  return result;
}

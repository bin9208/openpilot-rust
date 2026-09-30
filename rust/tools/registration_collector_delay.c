#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

int ftruncate(int fd, off_t length) {
  int (*original)(int, off_t) = dlsym(RTLD_NEXT, "ftruncate");
  if (!original) return -1;
  int result = original(fd, length);
  char command[4096] = {0};
  FILE *file = fopen("/proc/self/cmdline", "rb");
  if (!file) return result;
  size_t size = fread(command, 1, sizeof(command) - 1, file);
  fclose(file);
  for (size_t i = 0; i < size; ++i) if (command[i] == '\0') command[i] = ' ';
  if (!strstr(command, "logmessaged_reference.py")) return result;
  char descriptor[64];
  char path[4096] = {0};
  snprintf(descriptor, sizeof(descriptor), "/proc/self/fd/%d", fd);
  ssize_t count = readlink(descriptor, path, sizeof(path) - 1);
  if (result == 0 && count > 0 && strstr(path, "/dev/shm/msgq_") && strstr(path, "/errorLogMessage")) {
    fputs("registration fixture: pausing before error publisher initialization\n", stderr);
    usleep(800000);
  }
  return result;
}

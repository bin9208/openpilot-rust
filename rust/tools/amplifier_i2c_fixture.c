// Isolated Linux I2C UAPI oracle. Only an explicitly selected regular file is emulated.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/i2c-dev.h>
#include <linux/i2c.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static unsigned char registers[256];
static unsigned event_index;
static int initialized;
static void initialize(void) {
  if (initialized) return;
  initialized = 1;
  unsigned seed = (unsigned)strtoul(getenv("AMP_SEED") ?: "0", NULL, 10);
  for (unsigned i = 0; i < 256; ++i) registers[i] = (unsigned char)(i * 37 + seed);
}
static int selected(int fd) {
  const char *path = getenv("AMP_DEVICE_PATH");
  struct stat expected, actual;
  return path && stat(path, &expected) == 0 && fstat(fd, &actual) == 0 && S_ISREG(actual.st_mode)
      && expected.st_dev == actual.st_dev && expected.st_ino == actual.st_ino;
}
static int fault(unsigned index) {
  const char *configuration = getenv("AMP_FAULTS");
  if (!configuration) return 0;
  char *copy = strdup(configuration), *save = NULL;
  int result = 0;
  for (char *part = strtok_r(copy, ",", &save); part; part = strtok_r(NULL, ",", &save)) {
    unsigned position; int error;
    if (sscanf(part, "%u:%d", &position, &error) == 2 && position == index) result = error;
  }
  free(copy);
  return result;
}
static FILE *trace(void) { return fopen(getenv("AMP_TRACE_PATH"), "a"); }
int ioctl(int fd, unsigned long request, ...) {
  va_list args;
  va_start(args, request);
  unsigned long argument = va_arg(args, unsigned long);
  va_end(args);
  if (request != I2C_SLAVE_FORCE && request != I2C_SMBUS) {
    int (*next)(int, unsigned long, ...) = dlsym(RTLD_NEXT, "ioctl");
    return next(fd, request, argument);
  }
  if (!selected(fd)) { errno = EPERM; return -1; }
  initialize();
  unsigned index = event_index++;
  int error = fault(index);
  FILE *log = trace();
  if (request == I2C_SLAVE_FORCE) {
    fprintf(log, "{\"index\":%u,\"kind\":\"force\",\"request\":%lu,\"address\":%lu,\"errno\":%d}\n", index, request, argument, error);
    if (argument != 16) abort();
  } else {
    struct i2c_smbus_ioctl_data *data = (void *)argument;
    if (!data || !data->data || data->size != I2C_SMBUS_BYTE_DATA || data->read_write > 1) abort();
    unsigned char byte = data->read_write == I2C_SMBUS_READ ? registers[data->command] : data->data->byte;
    fprintf(log, "{\"index\":%u,\"kind\":\"smbus\",\"request\":%lu,\"read_write\":%u,\"command\":%u,\"size\":%u,\"byte\":%u,\"errno\":%d,\"struct_size\":%zu,\"union_size\":%zu,\"aligned\":%s}\n", index, request, data->read_write, data->command, data->size, byte, error, sizeof(*data), sizeof(*data->data), ((uintptr_t)data->data % _Alignof(union i2c_smbus_data)) ? "false" : "true");
    if (!error) {
      if (data->read_write == I2C_SMBUS_READ) data->data->byte = byte;
      else registers[data->command] = byte;
    }
  }
  fclose(log);
  if (error) { errno = error; return -1; }
  return 0;
}
int close(int fd) {
  int (*next)(int) = dlsym(RTLD_NEXT, "close");
  if (!selected(fd)) return next(fd);
  initialize();
  unsigned index = event_index++;
  int error = fault(index);
  int result = next(fd);
  int closed = fcntl(fd, F_GETFD) == -1 && errno == EBADF;
  FILE *log = trace();
  fprintf(log, "{\"index\":%u,\"kind\":\"close\",\"errno\":%d,\"closed\":%s}\n", index, error, closed ? "true" : "false");
  fclose(log);
  if (error) { errno = error; return -1; }
  return result;
}
__attribute__((destructor)) static void save_state(void) {
  const char *path = getenv("AMP_STATE_PATH");
  if (!path) return;
  initialize();
  FILE *output = fopen(path, "wb");
  if (output) { fwrite(registers, 1, sizeof(registers), output); fclose(output); }
}

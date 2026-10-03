#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

static int owned_fd = -1;
static int opens = 0;
static int read_injected = 0;

static int matches(const char *path) {
  const char *target = getenv("INPUT_FIXTURE_PATH");
  return target && path && strcmp(path, target) == 0;
}

static void record(const char *operation, long a, long b) {
  const char *path = getenv("INPUT_FIXTURE_LOG");
  if (!path) return;
  int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
  int (*real_close)(int) = dlsym(RTLD_NEXT, "close");
  int fd = real_open(path, O_WRONLY | O_APPEND | O_CREAT | O_CLOEXEC, 0600);
  if (fd >= 0) {
    dprintf(fd, "%s %ld %ld\n", operation, a, b);
    real_close(fd);
  }
}

int open64(const char *path, int flags, ...) {
  int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
  mode_t mode = 0;
  if (flags & O_CREAT) {
    va_list args;
    va_start(args, flags);
    mode = va_arg(args, mode_t);
    va_end(args);
  }
  if (!matches(path)) return real_open(path, flags, mode);
  opens++;
  record("open", flags, opens);
  if (getenv("INPUT_FIXTURE_DENIED") && opens == 1) { errno = EACCES; return -1; }
  const char *actual = getenv("INPUT_FIXTURE_FIFO");
  owned_fd = real_open(actual, flags, mode);
  return owned_fd;
}

int ioctl(int fd, unsigned long request, ...) {
  int (*real_ioctl)(int, unsigned long, ...) = dlsym(RTLD_NEXT, "ioctl");
  va_list args;
  va_start(args, request);
  if (fd == owned_fd && request == 0x40044590UL) {
    int value = va_arg(args, int);
    va_end(args);
    record("grab", value, 0);
    if (getenv("INPUT_FIXTURE_FAIL_GRAB")) { errno = EBUSY; return -1; }
    return value == 1 ? 0 : (errno = EINVAL, -1);
  }
  void *argument = va_arg(args, void *);
  va_end(args);
  if (fd == owned_fd && request == 0x400445a0UL) {
    int value;
    memcpy(&value, argument, sizeof(value));
    record("clock", value, 0);
    if (getenv("INPUT_FIXTURE_FAIL_CLOCK")) { errno = EINVAL; return -1; }
    return value == 1 ? 0 : (errno = EINVAL, -1);
  }
  return real_ioctl(fd, request, argument);
}

int close(int fd) {
  int (*real_close)(int) = dlsym(RTLD_NEXT, "close");
  if (fd == owned_fd) { record("close", 0, 0); owned_fd = -1; }
  return real_close(fd);
}

ssize_t read(int fd, void *buffer, size_t count) {
  ssize_t (*real_read)(int, void *, size_t) = dlsym(RTLD_NEXT, "read");
  const char *stage = getenv("INPUT_FIXTURE_READ_STAGE");
  if (fd == owned_fd && stage && !read_injected &&
      ((strcmp(stage, "drain") == 0 && count == 24 * 64) || (strcmp(stage, "active") == 0 && count == 24 * 128))) {
    read_injected = 1;
    int interrupted = getenv("INPUT_FIXTURE_READ_INTR") != NULL;
    record(interrupted ? "read-eintr" : "read-eio", count, 0);
    errno = interrupted ? EINTR : EIO;
    return -1;
  }
  return real_read(fd, buffer, count);
}

int statx(int dirfd, const char *path, int flags, unsigned int mask, struct statx *result) {
  int (*real_statx)(int, const char *, int, unsigned int, struct statx *) = dlsym(RTLD_NEXT, "statx");
  int status = real_statx(dirfd, matches(path) ? getenv("INPUT_FIXTURE_FIFO") : path, flags, mask, result);
  if (status == 0 && matches(path) && !getenv("INPUT_FIXTURE_NOT_CHAR")) result->stx_mode = (result->stx_mode & ~S_IFMT) | S_IFCHR;
  return status;
}

int fstatat64(int dirfd, const char *path, struct stat64 *result, int flags) {
  int (*real_stat)(int, const char *, struct stat64 *, int) = dlsym(RTLD_NEXT, "fstatat64");
  int status = real_stat(dirfd, matches(path) ? getenv("INPUT_FIXTURE_FIFO") : path, result, flags);
  if (status == 0 && matches(path) && !getenv("INPUT_FIXTURE_NOT_CHAR")) result->st_mode = (result->st_mode & ~S_IFMT) | S_IFCHR;
  return status;
}

int stat64(const char *path, struct stat64 *result) {
  int (*real_stat)(const char *, struct stat64 *) = dlsym(RTLD_NEXT, "stat64");
  int status = real_stat(matches(path) ? getenv("INPUT_FIXTURE_FIFO") : path, result);
  if (status == 0 && matches(path) && !getenv("INPUT_FIXTURE_NOT_CHAR")) result->st_mode = (result->st_mode & ~S_IFMT) | S_IFCHR;
  return status;
}

int __xstat64(int version, const char *path, struct stat64 *result) {
  int (*real_stat)(int, const char *, struct stat64 *) = dlsym(RTLD_NEXT, "__xstat64");
  int status = real_stat(version, matches(path) ? getenv("INPUT_FIXTURE_FIFO") : path, result);
  if (status == 0 && matches(path) && !getenv("INPUT_FIXTURE_NOT_CHAR")) result->st_mode = (result->st_mode & ~S_IFMT) | S_IFCHR;
  return status;
}

int __fxstatat64(int version, int dirfd, const char *path, struct stat64 *result, int flags) {
  int (*real_stat)(int, int, const char *, struct stat64 *, int) = dlsym(RTLD_NEXT, "__fxstatat64");
  int status = real_stat(version, dirfd, matches(path) ? getenv("INPUT_FIXTURE_FIFO") : path, result, flags);
  if (status == 0 && matches(path) && !getenv("INPUT_FIXTURE_NOT_CHAR")) result->st_mode = (result->st_mode & ~S_IFMT) | S_IFCHR;
  return status;
}

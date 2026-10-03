#define _GNU_SOURCE
#include <arpa/inet.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static ssize_t (*real_send)(int, const void *, size_t, int);
static ssize_t (*real_recv)(int, void *, size_t, int);
static pthread_once_t initialized = PTHREAD_ONCE_INIT;
static atomic_int active_fd = -1;
static atomic_int remaining = 2;

static void initialize(void) {
  real_send = dlsym(RTLD_NEXT, "send");
  real_recv = dlsym(RTLD_NEXT, "recv");
  if (!real_send || !real_recv) _exit(120);
}

static int loopback(int fd) {
  struct sockaddr_in address;
  socklen_t length = sizeof(address);
  return getpeername(fd, (struct sockaddr *)&address, &length) == 0 &&
    address.sin_family == AF_INET && ntohl(address.sin_addr.s_addr) == INADDR_LOOPBACK;
}

ssize_t send(int fd, const void *data, size_t length, int flags) {
  pthread_once(&initialized, initialize);
  if (length >= 4 && loopback(fd)) {
    if (!memcmp(data, "PUT ", 4)) atomic_store(&active_fd, fd);
    if (!memcmp(data, "GET ", 4)) atomic_store(&active_fd, -1);
  }
  return real_send(fd, data, length, flags);
}

ssize_t recv(int fd, void *data, size_t length, int flags) {
  pthread_once(&initialized, initialize);
  if (!(flags & MSG_PEEK) && fd == atomic_load(&active_fd) && loopback(fd)) {
    int count = atomic_load(&remaining);
    if (count > 0 && atomic_compare_exchange_strong(&remaining, &count, count - 1)) {
      const char *path = getenv("HTTP_EINTR_TRACE");
      if (!path) _exit(121);
      int trace = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
      if (trace < 0 || write(trace, "PUT recv EINTR\n", 15) != 15 || close(trace) != 0) _exit(122);
      errno = EINTR;
      return -1;
    }
  }
  return real_recv(fd, data, length, flags);
}

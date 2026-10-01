#define _GNU_SOURCE
#include <arpa/inet.h>
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <unistd.h>

int connect(int fd, const struct sockaddr *address, socklen_t length) {
  int (*original)(int, const struct sockaddr *, socklen_t) = dlsym(RTLD_NEXT, "connect");
  if (original == NULL) {
    errno = ENOSYS;
    return -1;
  }
  int result = original(fd, address, length);
  int saved_errno = errno;
  const char *configured = getenv("ATHENA_FIXTURE_PORT");
  static int interrupted = 0;
  if (!interrupted && configured != NULL && address != NULL &&
      length >= sizeof(struct sockaddr_in) && address->sa_family == AF_INET) {
    const struct sockaddr_in *ipv4 = (const struct sockaddr_in *)address;
    if (ntohs(ipv4->sin_port) == strtol(configured, NULL, 10) &&
        (result == 0 || saved_errno == EINPROGRESS)) {
      interrupted = 1;
      const char marker[] = "fixture: connected socket interrupted\n";
      if (write(STDERR_FILENO, marker, sizeof(marker) - 1) < 0) {
        return -1;
      }
      errno = EINTR;
      return -1;
    }
  }
  errno = saved_errno;
  return result;
}

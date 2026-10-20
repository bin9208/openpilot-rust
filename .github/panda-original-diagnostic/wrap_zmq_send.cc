#include <cerrno>
#include <cstdint>
#include <cstdlib>
#include <fcntl.h>
#include <sys/uio.h>
#include <unistd.h>

extern "C" int __real_zmq_send(void *, const void *, size_t, int);

// Diagnostic-only linker wrapper. One append syscall contains the header and
// the exact byte sequence passed to the unchanged C++ logger.
extern "C" int __wrap_zmq_send(void *socket, const void *data, size_t length, int flags) {
  const int result = __real_zmq_send(socket, data, length, flags);
  const int saved_errno = errno;
  const char *path = std::getenv("PANDA_DIAG_SEND");
  if (path != nullptr && length <= 2 * 1024 * 1024) {
    struct Header {
      uint64_t length;
      int32_t result;
      int32_t error;
      int32_t flags;
      uint32_t magic;
    } header{length, result, saved_errno, flags, 0x50444c31};
    static_assert(sizeof(Header) == 24);
    const int fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
    if (fd >= 0) {
      const iovec parts[] = {{&header, sizeof(header)}, {const_cast<void *>(data), length}};
      // A short diagnostic write is rejected by the parser; it never changes
      // the original send's result or errno.
      (void)writev(fd, parts, 2);
      (void)close(fd);
    }
  }
  errno = saved_errno;
  return result;
}

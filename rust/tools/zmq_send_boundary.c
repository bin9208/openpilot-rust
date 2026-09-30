/* Test-only libzmq send boundary. No production build links this file. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

typedef int (*send_fn)(void *, void *, int);
typedef void *(*data_fn)(void *);
typedef size_t (*size_fn)(const void *);
#ifdef ZMQ_LINK_WRAP
extern int __real_zmq_msg_send(void *, void *, int);
extern void *zmq_msg_data(void *);
extern size_t zmq_msg_size(const void *);
#define ENTRY __wrap_zmq_msg_send
#else
#define ENTRY zmq_msg_send
#endif
static _Atomic unsigned attempt;

static int selected_error(unsigned index) {
  const char *sequence = getenv("ZMQ_TEST_SEND_ERRORS");
  if (!sequence) return 0;
  for (unsigned at = 0; *sequence; ++at) {
    char *end;
    long error = strtol(sequence, &end, 10);
    if (end == sequence) return 0;
    if (at == index) return (int)error;
    if (*end != ',') return 0;
    sequence = end + 1;
  }
  return 0;
}

static void record(unsigned index, int injected, int result, int error, int flags,
                   const unsigned char *packet, size_t size, size_t captured) {
  const char *path = getenv("ZMQ_TEST_SEND_TRACE");
  if (!path) return;
  char text[9000];
  int count = snprintf(text, sizeof(text), "%d\t%ld\t%u\t%d\t%d\t%d\t%d\t%zu\t",
                       getpid(), syscall(SYS_gettid), index, injected, result, error, flags, size);
  for (size_t index = 0; index < captured; ++index)
    count += snprintf(text + count, sizeof(text) - (size_t)count, "%02x", packet[index]);
  text[count++] = '\n';
  int fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
  if (fd >= 0) {
    size_t written = 0;
    while (written < (size_t)count) {
      ssize_t result = write(fd, text + written, (size_t)count - written);
      if (result > 0) written += (size_t)result;
      else if (result < 0 && errno == EINTR) continue;
      else break;
    }
    close(fd);
  }
}

int ENTRY(void *message, void *socket, int flags) {
#ifdef ZMQ_LINK_WRAP
  send_fn send = __real_zmq_msg_send;
  data_fn data = zmq_msg_data;
  size_fn size = zmq_msg_size;
#else
  const char *library = getenv("ZMQ_TEST_REAL_LIBRARY");
  void *handle = library ? dlopen(library, RTLD_NOW | RTLD_NOLOAD) : RTLD_NEXT;
  if (library && !handle) _exit(97);
  send_fn send = (send_fn)dlsym(handle, "zmq_msg_send");
  data_fn data = (data_fn)dlsym(handle, "zmq_msg_data");
  size_fn size = (size_fn)dlsym(handle, "zmq_msg_size");
  if (!send || !data || !size) _exit(97);
#endif
  const char *match = getenv("ZMQ_TEST_SEND_MATCH");
  size_t length = size(message);
  int selected = match && memmem(data(message), length, match, strlen(match));
  if (!selected) return send(message, socket, flags);
  unsigned char copy[4096];
  size_t captured = length < sizeof(copy) ? length : sizeof(copy);
  memcpy(copy, data(message), captured);
  unsigned index = atomic_fetch_add(&attempt, 1);
  int fault = selected_error(index);
  int result;
  if (fault) { errno = fault; result = -1; }
  else result = send(message, socket, flags);
  int saved_errno = errno;
  record(index, fault != 0, result, result < 0 ? saved_errno : 0, flags, copy, length, captured);
  errno = saved_errno;
  return result;
}

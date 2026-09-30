#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

typedef int (*receive_fn)(void *, void *, int);
typedef int (*option_fn)(void *, int, void *, size_t *);
#ifdef ZMQ_LINK_WRAP
extern int __real_zmq_msg_recv(void *, void *, int);
extern int zmq_getsockopt(void *, int, void *, size_t *);
#define ENTRY __wrap_zmq_msg_recv
#else
#define ENTRY zmq_msg_recv
#endif
static unsigned attempt;

static int selected_error(unsigned index) {
  const char *sequence = getenv("ZMQ_TEST_RECV_ERRORS");
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

int ENTRY(void *message, void *socket, int flags) {
#ifdef ZMQ_LINK_WRAP
  receive_fn receive = __real_zmq_msg_recv;
  option_fn option = zmq_getsockopt;
#else
  const char *library = getenv("ZMQ_TEST_REAL_LIBRARY");
  void *handle = library ? dlopen(library, RTLD_NOW | RTLD_NOLOAD) : RTLD_NEXT;
  if (library && !handle) _exit(97);
  receive_fn receive = (receive_fn)dlsym(handle, "zmq_msg_recv");
  option_fn option = (option_fn)dlsym(handle, "zmq_getsockopt");
  if (!receive || !option) _exit(97);
#endif
  int kind = 0;
  size_t size = sizeof(kind);
  /* ZMQ_TYPE=16, ZMQ_PULL=7: isolate the metric consumer from other sockets. */
  if (!getenv("ZMQ_TEST_RECV_ERRORS") || option(socket, 16, &kind, &size) || kind != 7)
    return receive(message, socket, flags);
  unsigned index = attempt++;
  int fault = selected_error(index);
  int result;
  if (fault) { errno = fault; result = -1; }
  else result = receive(message, socket, flags);
  int saved = errno;
  const char *path = getenv("ZMQ_TEST_RECV_TRACE");
  if (path) {
    FILE *trace = fopen(path, "a");
    if (trace) {
      fprintf(trace, "%u\t%d\t%d\t%d\t%d\n", index, fault, result < 0 ? saved : 0, result, flags);
      fclose(trace);
    }
  }
  errno = saved;
  return result;
}

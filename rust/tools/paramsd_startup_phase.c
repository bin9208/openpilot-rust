#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

static int (*original_sleep)(const struct timespec *, struct timespec *);
static int (*original_poll)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *);
static const char *position_path;
static int trace_fd = -1;
static int ready_fd = -1;
static int release_fd = -1;
static unsigned completed;
static int gated;
static int loop_started;
static _Thread_local int sleep_active;

__attribute__((constructor)) static void initialize(void) {
  original_sleep = dlsym(RTLD_NEXT, "nanosleep");
  original_poll = dlsym(RTLD_NEXT, "ppoll");
  if (!original_sleep || !original_poll) _exit(125);
  const char *prefix = getenv("OPENPILOT_PREFIX");
  const char *trace = getenv("PARAMSD_PHASE_TRACE");
  position_path = getenv("PARAMSD_PHASE_POSITION");
  if (!prefix || strncmp(prefix, "rust-probe-params-", 18) || !trace || !position_path) return;
  trace_fd = open(trace, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
  if (trace_fd < 0) _exit(125);
  const char *ready = getenv("PARAMSD_PHASE_READY_FD");
  const char *release = getenv("PARAMSD_PHASE_RELEASE_FD");
  if (ready && release) {
    ready_fd = atoi(ready);
    release_fd = atoi(release);
  }
}

static int main_loop(void) {
  if (trace_fd < 0 || syscall(SYS_gettid) != getpid()) return 0;
  if (!loop_started && access(position_path, F_OK) == -1 && errno == ENOENT) loop_started = 1;
  return loop_started;
}

static void observe(const char *event, const char *operation, int result) {
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now)) _exit(125);
  if (dprintf(trace_fd, "{\"event\":\"%s\",\"operation\":\"%s\",\"completed\":%u,\"result\":%d,\"sec\":%ld,\"ns\":%ld}\n",
      event, operation, completed, result, now.tv_sec, now.tv_nsec) < 0) _exit(125);
}

static void enter(const char *operation) {
  observe("enter", operation, 0);
  if (completed == 1 && !gated && ready_fd >= 0 && release_fd >= 0) {
    gated = 1;
    observe("gate", operation, 0);
    if (write(ready_fd, "R", 1) != 1) _exit(125);
    char release;
    ssize_t count;
    do { count = read(release_fd, &release, 1); } while (count == -1 && errno == EINTR);
    if (count != 1 || release != 'R') _exit(125);
    observe("release", operation, 0);
  }
}

static int positive_poll(const struct timespec *timeout) {
  return timeout && timeout->tv_sec == 0 && timeout->tv_nsec > 0 && timeout->tv_nsec <= 100000000;
}

int ppoll(struct pollfd *fds, nfds_t count, const struct timespec *timeout, const sigset_t *mask) {
  int previous_error = errno;
  int observed = count == 0 && positive_poll(timeout) && main_loop();
  if (observed) enter("ppoll");
  errno = previous_error;
  int result = original_poll(fds, count, timeout, mask);
  int error = errno;
  if (observed) {
    if (result == 0) completed++;
    observe("return", "ppoll", result == -1 ? -error : result);
  }
  errno = error;
  return result;
}

int nanosleep(const struct timespec *request, struct timespec *remaining) {
  int previous_error = errno;
  int full = request && request->tv_sec == 0 && request->tv_nsec == 100000000;
  int observed = main_loop() && (full || sleep_active);
  if (observed) {
    if (!sleep_active) enter("nanosleep");
    sleep_active = 1;
  }
  errno = previous_error;
  int result = original_sleep(request, remaining);
  int error = errno;
  if (observed) {
    if (result == 0) completed++;
    observe("return", "nanosleep", result == -1 ? -error : result);
    if (result == 0 || error != EINTR) sleep_active = 0;
  }
  errno = error;
  return result;
}

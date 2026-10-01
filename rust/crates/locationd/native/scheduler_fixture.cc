#include "scheduler.h"
#include <sched.h>
#include <cerrno>
#include <cassert>
#include <cstdio>
#include <system_error>

static int calls = 0;
static int fail_at = 0;
extern "C" int sched_setscheduler(pid_t pid, int policy, const sched_param* priority) noexcept {
  assert(calls++ == 0);
  assert(pid == 0 && policy == SCHED_FIFO && priority->sched_priority == 5);
  if (fail_at == 1) { errno = EPERM; return -1; }
  return 0;
}
extern "C" int sched_setaffinity(pid_t pid, size_t size, const cpu_set_t* cores) noexcept {
  assert(calls++ == 1);
  assert(pid == 0 && size == sizeof(cpu_set_t));
  for (int core = 0; core < CPU_SETSIZE; ++core) assert(bool(CPU_ISSET(core, cores)) == (core < 4));
  if (fail_at == 2) { errno = EPERM; return -1; }
  return 0;
}
int main() {
  locationd::configure_scheduler(true);
  assert(calls == 0);
  for (int fault = 0; fault < 3; ++fault) {
    calls = 0;
    fail_at = fault;
    bool error = false;
    try { locationd::configure_scheduler(false); }
    catch (const std::system_error& e) { error = true; assert(e.code().value() == EPERM); }
    assert(error == (fault != 0));
    assert(calls == (fault == 1 ? 1 : 2));
  }
  std::puts("PASS scheduling boundary: PC bypass, FIFO5, cores0..3, order and syscall errors");
}

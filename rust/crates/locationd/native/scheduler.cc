#include "scheduler.h"
#include <sched.h>
#include <cerrno>
#include <system_error>

namespace locationd {
void configure_scheduler(bool pc) {
  if (pc) return;
  sched_param priority{};
  priority.sched_priority = 5;
  if (sched_setscheduler(0, SCHED_FIFO, &priority) < 0) throw std::system_error(errno, std::generic_category());
  cpu_set_t cores;
  CPU_ZERO(&cores);
  for (int core = 0; core < 4; ++core) CPU_SET(core, &cores);
  if (sched_setaffinity(0, sizeof(cores), &cores) < 0) throw std::system_error(errno, std::generic_category());
}
}

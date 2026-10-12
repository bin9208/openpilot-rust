#include "updater_scheduling.h"
#include "openpilot-ui-application/src/agnos_updater/scheduling/bridge.rs.h"
#include <cerrno>
#include <sched.h>
namespace agnos_updater {
Result set_fifo(int32_t priority) noexcept {
  const sched_param param{priority};
  const int result = sched_setscheduler(0, SCHED_FIFO, &param);
  return {result < 0 ? errno : 0};
}
}

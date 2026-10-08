#include "scheduling.h"
#include "openpilot-ui-application/src/scheduling/bridge.rs.h"
#include <cerrno>
#include <sched.h>
namespace product_ui {
SchedulerResult get_policy(int32_t tid) noexcept {
  const int result = sched_getscheduler(tid);
  return {result, result < 0 ? errno : 0};
}
SchedulerResult set_other(int32_t tid) noexcept {
  const sched_param param{};
  const int result = sched_setscheduler(tid, SCHED_OTHER, &param);
  return {result, result < 0 ? errno : 0};
}
}

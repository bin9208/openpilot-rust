#pragma once
#include <cstdint>
namespace product_ui {
struct SchedulerResult;
SchedulerResult get_policy(int32_t tid) noexcept;
SchedulerResult set_other(int32_t tid) noexcept;
}

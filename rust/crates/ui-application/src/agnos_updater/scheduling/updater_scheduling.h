#pragma once
#include <cstdint>
namespace agnos_updater {
struct Result;
Result set_fifo(int32_t priority) noexcept;
}

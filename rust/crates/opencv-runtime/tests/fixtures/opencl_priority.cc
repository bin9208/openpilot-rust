// Standalone boundary tests: no OpenCV/OpenCL runtime, no device access.
#include "opencl_priority.h"
#include <cassert>
#include <cstring>
#include <stdexcept>
#include <vector>
using namespace openpilot_opencv::opencl_abi;
namespace {
enum class Mode { valid, reordered, ignored, wrong, info_size_error, info_data_error,
                  truncated, oversized, missing_terminator, early_terminator, duplicate, even_size, create_error, null_context };
Mode mode;
int releases;
const auto device = reinterpret_cast<Device>(0x1000);
const auto context = reinterpret_cast<Context>(0x2000);
Context create(const Properties* props, Uint count, const Device* devices, Notify notify, void* user, Int* error) {
  // Literal expectations from the SDK header, independent of adapter constants.
  assert(props[0] == 0x40C9 && props[1] == 0x40CC && props[2] == 0);
  assert(count == 1 && devices[0] == device && !notify && !user);
  *error = mode == Mode::create_error ? -5 : 0;
  return mode == Mode::null_context ? nullptr : context;
}
Int info(Context input, Uint property, std::size_t size, void* data, std::size_t* returned) {
  assert(input == context && property == 0x1082);
  std::vector<Properties> props{0x40C9, 0x40CC, 0};
  if (mode == Mode::reordered) props = {0x1084, 123, 0x40C9, 0x40CC, 0};
  if (mode == Mode::ignored) props = {0x1084, 123, 0};
  if (mode == Mode::wrong) props[1] = 0x40CB;
  if (mode == Mode::early_terminator) props = {0x40C9, 0x40CC, 0, 999, 0};
  if (mode == Mode::duplicate) props = {0x40C9, 0x40CC, 0x40C9, 0x40CB, 0};
  if (mode == Mode::even_size) props = {0x40C9, 0x40CC, 0, 0};
  if (mode == Mode::missing_terminator) props.back() = 456;
  if (!data) {
    if (mode == Mode::info_size_error) return -30;
    *returned = mode == Mode::truncated ? 1 : mode == Mode::oversized ? 8192 : props.size() * sizeof(Properties);
    return 0;
  }
  if (mode == Mode::info_data_error) return -30;
  assert(size == props.size() * sizeof(Properties));
  std::memcpy(data, props.data(), size);
  return 0;
}
Int release(Context value) { assert(value == context); ++releases; return 0; }
}
int main() {
  for (auto scenario : {Mode::valid, Mode::reordered, Mode::ignored, Mode::wrong,
       Mode::info_size_error, Mode::info_data_error, Mode::truncated,
       Mode::oversized, Mode::missing_terminator, Mode::early_terminator, Mode::duplicate,
       Mode::even_size, Mode::create_error, Mode::null_context}) {
    mode = scenario;
    releases = 0;
    const bool valid = mode == Mode::valid || mode == Mode::reordered;
    bool succeeded = false;
    try {
      auto owned = make_low_priority_context(device, create, info, release);
      assert(owned.get() == context && releases == 0);
      succeeded = true;
    } catch (const std::runtime_error&) {}
    assert(succeeded == valid);
    assert(releases == (mode == Mode::null_context ? 0 : 1));
  }
}

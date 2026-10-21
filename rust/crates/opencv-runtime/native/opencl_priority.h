#pragma once
#include <cstdint>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace openpilot_opencv::opencl_abi {
// Selected Linux OpenCL 1.2 ABI declarations, not an OpenCL implementation.
// Verified sources/tokens and Qualcomm scheduling semantics: PROVENANCE.md.
using Int = std::int32_t;
using Uint = std::uint32_t;
using Properties = std::intptr_t;
struct _cl_device_id;
struct _cl_context;
using Device = _cl_device_id*;
using Context = _cl_context*;
using Notify = void (*)(const char*, const void*, std::size_t, void*);
using Create = Context (*)(const Properties*, Uint, const Device*, Notify, void*, Int*);
using Info = Int (*)(Context, Uint, std::size_t, void*, std::size_t*);
using Release = Int (*)(Context);
constexpr Uint context_properties = 0x1082; // CL_CONTEXT_PROPERTIES
constexpr Properties qcom_priority = 0x40C9; // CL_CONTEXT_PRIORITY_HINT_QCOM
constexpr Properties qcom_low = 0x40CC; // CL_PRIORITY_HINT_LOW_QCOM
struct ContextDeleter {
  Release release;
  void operator()(Context context) const noexcept { release(context); }
};
using OwnedContext = std::unique_ptr<_cl_context, ContextDeleter>;

inline OwnedContext make_low_priority_context(Device device, Create create, Info info, Release release) {
  const Properties properties[] = {qcom_priority, qcom_low, 0};
  Int error = 0;
  OwnedContext owner(create(properties, 1, &device, nullptr, nullptr, &error), ContextDeleter{release});
  if (!owner || error != 0)
    throw std::runtime_error("QCOM low-priority clCreateContext failed: " + std::to_string(error));
  std::size_t size = 0;
  if (info(owner.get(), context_properties, 0, nullptr, &size) != 0 ||
      size < sizeof(properties) || size > 4096 || size % sizeof(Properties) || (size / sizeof(Properties)) % 2 != 1)
    throw std::runtime_error("QCOM priority: cannot verify context properties");
  std::vector<Properties> actual(size / sizeof(Properties));
  if (info(owner.get(), context_properties, size, actual.data(), nullptr) != 0 || actual.back() != 0)
    throw std::runtime_error("QCOM priority: cannot read context properties");
  bool verified = false;
  for (std::size_t i = 0; i + 1 < actual.size(); i += 2) {
    if (actual[i] == 0) throw std::runtime_error("QCOM priority: malformed context property list");
    if (actual[i] == qcom_priority) {
      if (verified || actual[i + 1] != qcom_low)
        throw std::runtime_error("QCOM priority: conflicting context priority properties");
      verified = true;
    }
  }
  if (!verified) throw std::runtime_error("QCOM priority: driver did not retain low-priority hint");
  return owner;
}
}

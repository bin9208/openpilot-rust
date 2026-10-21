#pragma once
#include <stdexcept>

namespace openpilot_opencv {
// Context import can wrap the same cl_device_id in a different Device::Impl.
// Pinned OpenCV's create(Context, Device) requires the context-owned wrapper.
// Keep validation/selection at the factory boundary so standalone tests cover
// the actual argument passed to OpenCLExecutionContext::create.
template <typename Context, typename Device, typename Create>
auto create_context_execution(const Context& context, const Device& selected, Create create) {
  if (context.empty() || context.ndevices() != 1)
    throw std::runtime_error("OpenCL FP32 requires a context owning exactly one device");
  const auto& owned_device = context.device(0);
  if (!owned_device.ptr() || owned_device.ptr() != selected.ptr())
    throw std::runtime_error("OpenCL FP32 context device differs from selected GPU");
  return create(context, owned_device);
}
}

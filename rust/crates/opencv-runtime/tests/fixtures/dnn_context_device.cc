// Adapter boundary regression only. Models the pinned OpenCV wrapper-identity
// requirement without loading OpenCV/OpenCL or claiming a real GPU was tested.
#include "dnn_context_device.h"
#include <cassert>
#include <cstddef>
#include <stdexcept>
#include <vector>
namespace {
struct Device {
  void* handle;
  int wrapper_id;
  void* ptr() const { return handle; }
};
struct Context {
  bool is_empty = false;
  std::vector<Device> devices;
  bool empty() const { return is_empty; }
  std::size_t ndevices() const { return devices.size(); }
  const Device& device(std::size_t index) const { return devices.at(index); }
};
int creates = 0;
int create(const Context& context, const Device& device) {
  ++creates;
  // Pinned OpenCV compares wrapper identity, not the raw OpenCL handle.
  if (device.wrapper_id != context.device(0).wrapper_id)
    throw std::runtime_error("device wrapper does not belong to context");
  assert(&device == &context.device(0));
  return device.wrapper_id;
}
void rejected(const Context& context, const Device& selected) {
  const int before = creates;
  bool threw = false;
  try { openpilot_opencv::create_context_execution(context, selected, create); }
  catch (const std::runtime_error&) { threw = true; }
  assert(threw && creates == before); // Reject before constructing a queue.
}
}
int main() {
  int handle_a = 0, handle_b = 0;
  const Device selected{&handle_a, 11};
  const Context imported{false, {{&handle_a, 22}}};
  assert(selected.ptr() == imported.device(0).ptr());
  assert(selected.wrapper_id != imported.device(0).wrapper_id);
  bool old_call_failed = false;
  try { create(imported, selected); }
  catch (const std::runtime_error&) { old_call_failed = true; }
  assert(old_call_failed);
  creates = 0;
  assert(openpilot_opencv::create_context_execution(imported, selected, create) == 22);
  assert(creates == 1);
  const Context already_owned{false, {selected}};
  assert(openpilot_opencv::create_context_execution(already_owned, selected, create) == 11);
  rejected(Context{true, {{&handle_a, 22}}}, selected);
  rejected(Context{false, {}}, selected);
  rejected(Context{false, {{&handle_a, 22}, {&handle_a, 33}}}, selected);
  rejected(Context{false, {{&handle_b, 22}}}, selected);
  rejected(Context{false, {{nullptr, 22}}}, Device{nullptr, 11});
}

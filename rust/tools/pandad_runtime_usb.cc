#include <algorithm>
#include <array>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <mutex>
#include <string>
#include <vector>
#include <unistd.h>
#include <libusb-1.0/libusb.h>
#include <json11/json11.hpp>
#include "panda/board/health.h"

using json11::Json;
struct libusb_context {};
struct libusb_device { int index; };
struct libusb_device_handle { int index; };

namespace {
struct Device {
  uint16_t safety = 0;
  uint16_t safety_parameter = 0;
  uint16_t alternative = 0;
  uint8_t power_save = 0;
  uint8_t fan = 0;
  size_t serial_offset = 0;
  unsigned can_reads = 0;
};
struct State {
  std::mutex mutex;
  libusb_context context;
  std::array<libusb_device, 4> devices{{{0}, {1}, {2}, {3}}};
  std::array<libusb_device_handle, 4> handles{{{0}, {1}, {2}, {3}}};
  std::array<libusb_device *, 5> pointers{};
  std::array<Device, 4> state;
  std::string config_path;
  std::ofstream trace;
  bool health_gate_passed = false;
  State() {
    const char *config = std::getenv("PANDA_FIXTURE_CONFIG");
    const char *log = std::getenv("PANDA_FIXTURE_TRACE");
    if (!config || !log) { std::fputs("owned Panda fixture paths are required\n", stderr); std::abort(); }
    config_path = config;
    trace.open(log, std::ios::app);
    if (!trace) std::abort();
  }
  Json config() {
    std::ifstream stream(config_path);
    std::string text((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
    std::string error;
    Json parsed = Json::parse(text, error);
    if (!error.empty()) { std::fputs("invalid Panda fixture config\n", stderr); std::abort(); }
    return parsed;
  }
  void record(Json value) { trace << value.dump() << std::endl; }
};
State &state() { static State value; return value; }
Json bytes(const uint8_t *input, size_t length) {
  Json::array result;
  for (size_t i = 0; i < length; ++i) result.emplace_back(input[i]);
  return result;
}
std::string serial(int index) {
  if (state().config()["supervisor"].bool_value()) return std::string(23, '0') + std::to_string(index);
  return "PANDA-FIXTURE-" + std::to_string(index);
}
int count(const Json &config) { return std::clamp(config["count"].is_null() ? 1 : config["count"].int_value(), 0, 4); }
template <typename T> int copy_packet(unsigned char *out, uint16_t length, const T &packet) {
  const size_t size = std::min<size_t>(length, sizeof(packet));
  std::memcpy(out, &packet, size);
  return static_cast<int>(size);
}
}

extern "C" {
int libusb_init(libusb_context **out) {
  auto &s = state(); std::lock_guard lock(s.mutex);
  *out = &s.context; s.record(Json::object{{"op", "init"}}); return 0;
}
int libusb_set_option(libusb_context *, libusb_option, ...) { return 0; }
void libusb_exit(libusb_context *) {
  auto &s = state(); std::lock_guard lock(s.mutex); s.record(Json::object{{"op", "exit"}});
}
ssize_t libusb_get_device_list(libusb_context *, libusb_device ***out) {
  auto &s = state(); std::lock_guard lock(s.mutex);
  const int length = count(s.config());
  for (int i = 0; i < length; ++i) s.pointers[i] = &s.devices[i];
  s.pointers[length] = nullptr; *out = s.pointers.data();
  s.record(Json::object{{"op", "list"}, {"count", length}}); return length;
}
void libusb_free_device_list(libusb_device **, int) {}
int libusb_get_device_descriptor(libusb_device *device, libusb_device_descriptor *out) {
  *out = {}; out->idVendor = 0x3801; out->idProduct = 0xddcc; out->iSerialNumber = 3;
  if (state().config()["supervisor"].bool_value()) out->bcdDevice = (device->index == 0 ? 6 : 7) << 8;
  return 0;
}
int libusb_open(libusb_device *device, libusb_device_handle **out) {
  auto &s = state(); std::lock_guard lock(s.mutex);
  *out = &s.handles[device->index]; s.record(Json::object{{"op", "open"}, {"index", device->index}}); return 0;
}
void libusb_close(libusb_device_handle *handle) {
  auto &s = state(); std::lock_guard lock(s.mutex); s.record(Json::object{{"op", "close"}, {"index", handle->index}});
}
int libusb_get_string_descriptor_ascii(libusb_device_handle *handle, uint8_t, unsigned char *out, int length) {
  const auto value = serial(handle->index); const int size = std::min(length, static_cast<int>(value.size()));
  std::memcpy(out, value.data(), size); return size;
}
int libusb_kernel_driver_active(libusb_device_handle *, int) { return 0; }
int libusb_detach_kernel_driver(libusb_device_handle *, int) { return 0; }
int libusb_set_configuration(libusb_device_handle *, int) { return 0; }
int libusb_set_auto_detach_kernel_driver(libusb_device_handle *, int) { return 0; }
int libusb_claim_interface(libusb_device_handle *, int) { return 0; }
int libusb_release_interface(libusb_device_handle *, int) { return 0; }
const char *libusb_strerror(int) { return "owned fixture disconnect"; }

int libusb_control_transfer(libusb_device_handle *handle, uint8_t kind, uint8_t request, uint16_t value,
                            uint16_t index, unsigned char *out, uint16_t length, unsigned int timeout) {
  auto &s = state(); std::lock_guard lock(s.mutex);
  const Json config = s.config();
  auto &device = s.state[handle->index];
  s.record(Json::object{{"op", "control"}, {"device", handle->index}, {"kind", kind}, {"request", request},
    {"value", value}, {"index", index}, {"length", length}, {"timeout", static_cast<double>(timeout)}});
  if (kind == 0x40) {
    if (request == 0xdc) { device.safety = value; device.safety_parameter = index; }
    if (request == 0xdf) device.alternative = value;
    if (request == 0xe7) device.power_save = value;
    if (request == 0xb1) device.fan = value;
    return 0;
  }
  if (request == 0xc1 && length > 0) { out[0] = handle->index == 0 ? 6 : 7; return 1; }
  if (config["supervisor"].bool_value() && request == 0xdd) {
    const uint8_t versions[] = {16, 4, 5};
    return copy_packet(out, length, versions);
  }
  if (config["supervisor"].bool_value() && request == 0xd6) {
    const std::string version = "owned-runtime-version";
    const auto size = std::min<size_t>(length, version.size());
    std::memcpy(out, version.data(), size);
    return static_cast<int>(size);
  }
  if (request == 0xd3 || request == 0xd4) { std::memset(out, 0x42, length); return length; }
  if (request == 0xd2) {
    if (!s.health_gate_passed) {
      s.health_gate_passed = true;
      const char *ready_fd = std::getenv("PANDA_FIXTURE_READY_FD");
      const char *release_fd = std::getenv("PANDA_FIXTURE_RELEASE_FD");
      if (ready_fd && release_fd) {
        char token = 'R';
        if (::write(std::stoi(ready_fd), &token, 1) != 1 ||
            ::read(std::stoi(release_fd), &token, 1) != 1 || token != 'G') std::abort();
      }
    }
    health_t health{};
    health.voltage_pkt = 12000; health.current_pkt = 1000;
    health.ignition_line_pkt = config["ignition"].bool_value();
    health.safety_mode_pkt = device.safety; health.safety_param_pkt = device.safety_parameter;
    health.power_save_enabled_pkt = device.power_save;
    health.alternative_experience_pkt = device.alternative;
    health.fan_power = device.fan;
    return copy_packet(out, length, health);
  }
  if (request == 0xc2) {
    can_health_t health{}; health.can_speed = 500; health.can_data_speed = 2000;
    return copy_packet(out, length, health);
  }
  if (request == 0xb2) { uint16_t rpm = 1234; return copy_packet(out, length, rpm); }
  if (request == 0xe0) {
    const std::string text = "SPI: fixture serial\n";
    const size_t size = std::min<size_t>(length, text.size() - device.serial_offset);
    std::memcpy(out, text.data() + device.serial_offset, size); device.serial_offset += size; return size;
  }
  std::memset(out, 0, length); return length;
}

int libusb_bulk_transfer(libusb_device_handle *handle, unsigned char endpoint, unsigned char *data,
                         int length, int *transferred, unsigned int timeout) {
  auto &s = state(); std::lock_guard lock(s.mutex);
  const Json config = s.config();
  auto &device = s.state[handle->index];
  if (endpoint == 3) {
    *transferred = length;
    s.record(Json::object{{"op", "write"}, {"device", handle->index}, {"endpoint", endpoint},
      {"timeout", static_cast<double>(timeout)}, {"data", bytes(data, length)}});
    return 0;
  }
  ++device.can_reads;
  if (!config["disconnect_after"].is_null() && device.can_reads >= static_cast<unsigned>(config["disconnect_after"].int_value())) {
    *transferred = 0; return LIBUSB_ERROR_NO_DEVICE;
  }
  const auto &packet = config["can_rx"].array_items();
  if (packet.size() > static_cast<size_t>(length)) std::abort();
  for (size_t i = 0; i < packet.size(); ++i) data[i] = packet[i].int_value();
  *transferred = packet.size();
  s.record(Json::object{{"op", "read"}, {"device", handle->index}, {"endpoint", endpoint},
    {"length", length}, {"timeout", static_cast<double>(timeout)}, {"count", *transferred}});
  return 0;
}
}

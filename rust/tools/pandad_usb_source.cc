#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <thread>
#include <vector>
#include <libusb-1.0/libusb.h>
#include <json11/json11.hpp>
#include "selfdrive/pandad/panda_comms.h"

using json11::Json;
struct libusb_context {};
struct libusb_device { size_t index; };
struct libusb_device_handle { size_t index; };
static libusb_context context;
static std::vector<libusb_device> devices;
static std::vector<libusb_device_handle> handles;
static std::vector<libusb_device *> pointers;
static Json request, operation;
static Json::array calls, logs;
static size_t step_index;
static uint64_t timestamp = 1000000000;
static std::atomic<bool> concurrent{false};
static std::atomic<unsigned> active_calls{0}, maximum_calls{0}, total_calls{0};

extern "C" int __wrap_clock_gettime(clockid_t, struct timespec *out) {
  timestamp += 200000000;
  out->tv_sec = timestamp / 1000000000;
  out->tv_nsec = timestamp % 1000000000;
  return 0;
}

void cloudlog_e(int level, const char *, int, const char *, const char *format, ...) {
  std::array<char, 1024> text{};
  va_list args;
  va_start(args, format);
  vsnprintf(text.data(), text.size(), format, args);
  va_end(args);
  logs.push_back(Json::object{{"level", level}, {"message", text.data()}});
}

Json bytes(const unsigned char *data, size_t length) {
  Json::array result;
  for (size_t i = 0; i < length; ++i) result.emplace_back(data[i]);
  return result;
}

const Json &next_step() {
  const auto &steps = operation["steps"].array_items();
  if (step_index >= steps.size()) throw std::runtime_error("transfer script exhausted");
  return steps[step_index++];
}

extern "C" {
int libusb_init(libusb_context **out) {
  calls.push_back("init");
  int ret = request["init"].int_value();
  *out = ret == 0 ? &context : nullptr;
  return ret;
}
int libusb_set_option(libusb_context *, libusb_option option, ...) {
  va_list args;
  va_start(args, option);
  int value = va_arg(args, int);
  va_end(args);
  calls.push_back(Json::array{"option", static_cast<int>(option), value});
  return 0;
}
void libusb_exit(libusb_context *) { calls.push_back("exit"); }
ssize_t libusb_get_device_list(libusb_context *, libusb_device ***out) {
  calls.push_back("list");
  if (!request["list_error"].is_null()) { *out = nullptr; return request["list_error"].int_value(); }
  *out = pointers.data();
  return devices.size();
}
void libusb_free_device_list(libusb_device **, int unref) { calls.push_back(Json::array{"free_list", unref}); }
int libusb_get_device_descriptor(libusb_device *device, libusb_device_descriptor *out) {
  auto config = request["devices"][device->index];
  calls.push_back(Json::array{"descriptor", static_cast<int>(device->index)});
  *out = {};
  out->idVendor = config["vendor"].int_value(); out->idProduct = config["product"].int_value(); out->iSerialNumber = 3;
  return 0;
}
int libusb_open(libusb_device *device, libusb_device_handle **out) {
  calls.push_back(Json::array{"open", static_cast<int>(device->index)});
  int ret = request["devices"][device->index]["open"].int_value();
  *out = ret < 0 ? nullptr : &handles[device->index];
  return ret;
}
void libusb_close(libusb_device_handle *handle) { calls.push_back(Json::array{"close", static_cast<int>(handle->index)}); }
int libusb_get_string_descriptor_ascii(libusb_device_handle *handle, uint8_t index, unsigned char *out, int length) {
  calls.push_back(Json::array{"serial", static_cast<int>(handle->index), index, length});
  auto config = request["devices"][handle->index];
  if (!config["serial_error"].is_null()) return config["serial_error"].int_value();
  auto serial = config["serial"].array_items();
  size_t count = std::min(serial.size(), static_cast<size_t>(length));
  for (size_t i = 0; i < count; ++i) out[i] = serial[i].int_value();
  return count;
}
int libusb_kernel_driver_active(libusb_device_handle *, int interface) { calls.push_back(Json::array{"active", interface}); return request["active"].int_value(); }
int libusb_detach_kernel_driver(libusb_device_handle *, int interface) { calls.push_back(Json::array{"detach", interface}); return 0; }
int libusb_set_configuration(libusb_device_handle *, int config) { calls.push_back(Json::array{"config", config}); return request["config"].int_value(); }
int libusb_claim_interface(libusb_device_handle *, int interface) { calls.push_back(Json::array{"claim", interface}); return request["claim"].int_value(); }
int libusb_release_interface(libusb_device_handle *, int interface) { calls.push_back(Json::array{"release", interface}); return 0; }
const char *libusb_strerror(int) { return "fixture USB error"; }
int libusb_control_transfer(libusb_device_handle *, uint8_t kind, uint8_t req, uint16_t value, uint16_t index, unsigned char *data, uint16_t length, unsigned int timeout) {
  if (concurrent.load()) {
    unsigned active = active_calls.fetch_add(1) + 1;
    unsigned maximum = maximum_calls.load();
    while (active > maximum && !maximum_calls.compare_exchange_weak(maximum, active)) {}
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    total_calls.fetch_add(1);
    active_calls.fetch_sub(1);
    return 0;
  }
  calls.push_back(Json::array{"control", kind, req, value, index, length, static_cast<double>(timeout)});
  const auto &step = next_step();
  if (step["data"].array_items().size() > length) throw std::runtime_error("control input exceeds buffer");
  for (size_t i = 0; i < step["data"].array_items().size(); ++i) data[i] = step["data"][i].int_value();
  return step["ret"].int_value();
}
int libusb_bulk_transfer(libusb_device_handle *, unsigned char endpoint, unsigned char *data, int length, int *transferred, unsigned int timeout) {
  calls.push_back(Json::array{"bulk", endpoint, length, static_cast<double>(timeout), endpoint & 0x80 ? Json() : bytes(data, length)});
  const auto &step = next_step();
  if (step["data"].array_items().size() > static_cast<size_t>(length)) throw std::runtime_error("bulk input exceeds buffer");
  for (size_t i = 0; i < step["data"].array_items().size(); ++i) data[i] = step["data"][i].int_value();
  *transferred = step["transferred"].int_value();
  return step["ret"].int_value();
}
}

static void begin(const Json &input) {
  request = input; calls.clear(); logs.clear();
  concurrent = false; active_calls = 0; maximum_calls = 0; total_calls = 0;
  devices.resize(request["devices"].array_items().size()); handles.resize(devices.size()); pointers.clear();
  for (size_t i = 0; i < devices.size(); ++i) { devices[i].index = handles[i].index = i; pointers.push_back(&devices[i]); }
  pointers.push_back(nullptr);
}

extern "C" void fixture_begin(const char *input) {
  std::string error;
  auto value = Json::parse(input, error);
  if (!error.empty()) std::abort();
  begin(value);
}
extern "C" void fixture_operation(const char *input) {
  std::string error;
  operation = Json::parse(input, error);
  if (!error.empty()) std::abort();
  step_index = 0;
}
extern "C" void fixture_log(int level, const char *message) {
  logs.push_back(Json::object{{"level", level}, {"message", message}});
}
extern "C" int fixture_consumed() { return step_index == operation["steps"].array_items().size(); }
extern "C" void fixture_concurrent_start() { concurrent = true; }
extern "C" const char *fixture_concurrent_result() {
  static std::string output;
  output = Json(Json::object{{"active", static_cast<int>(active_calls.load())}, {"maximum", static_cast<int>(maximum_calls.load())},
                            {"total", static_cast<int>(total_calls.load())}}).dump();
  return output.c_str();
}
extern "C" const char *fixture_finish() {
  static std::string output;
  output = Json(Json::object{{"calls", calls}, {"logs", logs}}).dump();
  return output.c_str();
}

#ifndef PANDA_USB_ABI_ONLY
Json run(const Json &input) {
  begin(input);
  if (input["mode"].string_value() == "list") {
    Json::array rows;
    for (int i = 0; i < input["repeats"].int_value(); ++i) {
      Json::array serials;
      for (const auto &serial : PandaUsbHandle::list()) serials.push_back(bytes(reinterpret_cast<const unsigned char *>(serial.data()), serial.size()));
      rows.push_back(serials);
    }
    return Json::object{{"failed", false}, {"serial", Json()}, {"results", rows}, {"calls", calls}, {"logs", logs}};
  }
  std::string serial;
  for (const auto &byte : request["serial"].array_items()) serial.push_back(byte.int_value());
  bool failed = false;
  Json::array results;
  Json hardware_serial;
  try {
    auto handle = std::make_unique<PandaUsbHandle>(serial);
    hardware_serial = bytes(reinterpret_cast<const unsigned char *>(handle->hw_serial.data()), handle->hw_serial.size());
    if (input["mode"].string_value().find("concurrent") == 0) {
      const bool unlocked = input["mode"].string_value() == "concurrent_unlocked";
      fixture_concurrent_start();
      std::atomic<bool> start{false};
      std::atomic<unsigned> ready{0}, errors{0};
      std::vector<std::thread> workers;
      for (int i = 0; i < input["threads"].int_value(); ++i) workers.emplace_back([&]() {
        ready.fetch_add(1);
        while (!start.load()) std::this_thread::yield();
        for (int j = 0; j < input["transfers"].int_value(); ++j) {
          int code = unlocked ? libusb_control_transfer(nullptr, 0x40, 0xdc, 1, 2, nullptr, 0, 5) : handle->control_write(0xdc, 1, 2, 5);
          if (code != 0) errors.fetch_add(1);
        }
      });
      while (ready.load() != static_cast<unsigned>(input["threads"].int_value())) std::this_thread::yield();
      start = true;
      for (auto &worker : workers) worker.join();
      std::string error;
      auto metrics = Json::parse(fixture_concurrent_result(), error);
      results.push_back(Json::object{{"metrics", metrics}, {"errors", static_cast<int>(errors.load())},
                                    {"connected", handle->connected.load()}, {"healthy", handle->comms_healthy.load()}});
    }
    for (const auto &next : request["operations"].array_items()) {
      operation = next; step_index = 0;
      auto name = operation["op"].string_value();
      std::vector<unsigned char> buffer(operation["length"].int_value(), 0);
      if (operation["data"].array_items().size() > buffer.size()) throw std::runtime_error("operation input exceeds buffer");
      for (size_t i = 0; i < operation["data"].array_items().size(); ++i) buffer[i] = operation["data"][i].int_value();
      int ret = 0;
      if (name == "disconnect") handle->connected = false;
      else if (name == "control_write") ret = handle->control_write(operation["request"].int_value(), operation["value"].int_value(), operation["index"].int_value(), static_cast<unsigned int>(operation["timeout"].number_value()));
      else if (name == "control_read") ret = handle->control_read(operation["request"].int_value(), operation["value"].int_value(), operation["index"].int_value(), buffer.data(), buffer.size(), static_cast<unsigned int>(operation["timeout"].number_value()));
      else if (name == "bulk_write") ret = handle->bulk_write(operation["endpoint"].int_value(), buffer.data(), buffer.size(), static_cast<unsigned int>(operation["timeout"].number_value()));
      else if (name == "bulk_read") ret = handle->bulk_read(operation["endpoint"].int_value(), buffer.data(), buffer.size(), static_cast<unsigned int>(operation["timeout"].number_value()));
      else throw std::runtime_error("unknown fixture operation");
      if (step_index != operation["steps"].array_items().size()) throw std::runtime_error("unconsumed transfer script");
      results.push_back(Json::object{{"result", ret}, {"data", bytes(buffer.data(), buffer.size())}, {"connected", handle->connected.load()}, {"healthy", handle->comms_healthy.load()}});
    }
  } catch (const std::runtime_error &error) {
    if (std::string(error.what()) != "Error connecting to panda over USB") throw;
    failed = true;
  }
  return Json::object{{"failed", failed}, {"serial", hardware_serial}, {"results", results}, {"calls", calls}, {"logs", logs}};
}

int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    std::string error;
    auto input = Json::parse(line, error);
    if (!error.empty()) throw std::runtime_error(error);
    std::cout << run(input).dump() << '\n';
  }
}
#endif

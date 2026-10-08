#include <array>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <vector>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/ioctl.h>
#include <linux/spi/spidev.h>
#include <json11/json11.hpp>
#include "selfdrive/pandad/panda_comms.h"

using json11::Json;
static Json scenario;
static Json::array calls, logs;
static size_t step_index;
static size_t defined_tx_bytes;
static size_t ignored_uninitialized_tx_bytes;
static uint64_t timestamp = 1000000000;
static uint64_t tick_ns = 100000;
static constexpr int fixture_fd = 517;
static bool fixture_active = false;
extern "C" int __real_sched_yield();

static Json bytes(const unsigned char *data, size_t length) {
  Json::array result;
  for (size_t i = 0; i < length; ++i) result.emplace_back(data[i]);
  return result;
}
static const Json &next_step() {
  const auto &steps = scenario["steps"].array_items();
  if (step_index >= steps.size()) throw std::runtime_error("SPI fixture script exhausted");
  return steps[step_index++];
}
static int result(const Json &step, int success = 0) {
  errno = step["errno"].int_value();
  return step["result"].is_null() ? success : step["result"].int_value();
}
static void descriptor(int fd) {
  if (fd != fixture_fd) throw std::runtime_error("unexpected SPI descriptor");
}
extern "C" int __wrap_clock_gettime(clockid_t, struct timespec *out) {
  timestamp += tick_ns;
  out->tv_sec = timestamp / 1000000000;
  out->tv_nsec = timestamp % 1000000000;
  return 0;
}
extern "C" int __wrap_open(const char *path, int flags, ...) {
  if (std::string(path) != "/dev/spidev0.0") throw std::runtime_error("unexpected open path");
  calls.push_back(Json::array{"open", path, flags});
  return result(scenario["open"], fixture_fd);
}
extern "C" int __wrap_stat(const char *path, struct stat *out) {
  if (std::string(path) != "/dev/spidev0.0") throw std::runtime_error("unexpected stat path");
  calls.push_back(Json::array{"stat", path});
  *out = {};
  return result(scenario["stat"]);
}
extern "C" int __wrap_close(int fd) {
  descriptor(fd); calls.push_back("close"); return result(scenario["close"]);
}
extern "C" int __wrap_flock(int fd, int operation) {
  descriptor(fd); calls.push_back(Json::array{"flock", operation}); return 0;
}
extern "C" int __wrap_sched_yield() {
  if (!fixture_active) return __real_sched_yield();
  calls.push_back("yield");
  return 0;
}
extern "C" int __wrap_usleep(useconds_t micros) {
  calls.push_back(Json::array{"sleep", static_cast<int>(micros)});
  timestamp += static_cast<uint64_t>(micros) * 1000;
  return 0;
}
extern "C" int __wrap_ioctl(int fd, unsigned long command, ...) {
  descriptor(fd);
  va_list args; va_start(args, command); void *arg = va_arg(args, void *); va_end(args);
  const auto &step = next_step();
  if (command != SPI_IOC_MESSAGE(1)) {
    const char *kind = command == SPI_IOC_WR_MODE ? "mode" : command == SPI_IOC_WR_MAX_SPEED_HZ ? "speed" : command == SPI_IOC_WR_BITS_PER_WORD ? "bits" : "unknown";
    if (step["kind"].string_value() != kind) throw std::runtime_error("unexpected SPI setup operation");
    uint32_t value = command == SPI_IOC_WR_BITS_PER_WORD ? *static_cast<uint8_t *>(arg) : *static_cast<uint32_t *>(arg);
    calls.push_back(Json::array{kind, static_cast<double>(value)});
    return result(step);
  }
  if (step["kind"].string_value() != "transfer") throw std::runtime_error("unexpected SPI transfer");
  const auto &transfer = *static_cast<spi_ioc_transfer *>(arg);
  auto *tx = reinterpret_cast<const uint8_t *>(transfer.tx_buf);
  auto *rx = reinterpret_cast<uint8_t *>(transfer.rx_buf);
  if (!step["tx"].is_null()) defined_tx_bytes = std::max(defined_tx_bytes, step["tx"].array_items().size());
  Json::array transmitted;
  for (size_t i = 0; tx && i < transfer.len; ++i) {
    if (i < defined_tx_bytes) transmitted.emplace_back(tx[i]);
    else { transmitted.emplace_back(); ++ignored_uninitialized_tx_bytes; }
  }
  Json data = tx ? Json(transmitted) : Json();
  calls.push_back(Json::array{"transfer", static_cast<int>(transfer.len), data});
  if (!step["tx"].is_null() && step["tx"] != data) throw std::runtime_error("SPI transmitted bytes differ");
  if (!step["length"].is_null() && step["length"].int_value() != static_cast<int>(transfer.len)) throw std::runtime_error("SPI transfer length differs");
  const auto &received = step["rx"].array_items();
  if (received.size() > transfer.len) throw std::runtime_error("fixture response larger than ioctl request");
  if (rx) {
    std::memset(rx, 0, transfer.len);
    for (size_t i = 0; i < received.size(); ++i) rx[i] = received[i].int_value();
  }
  timestamp += static_cast<uint64_t>(step["elapsed_ns"].number_value());
  return result(step, transfer.len);
}
void cloudlog_e(int level, const char *, int, const char *, const char *format, ...) {
  std::array<char, 2048> message{};
  va_list args; va_start(args, format); vsnprintf(message.data(), message.size(), format, args); va_end(args);
  logs.push_back(Json::object{{"level", level}, {"message", message.data()}});
}
static Json event() {
  const auto e = get_latest_panda_spi_error_event();
  return Json::object{{"sequence", static_cast<double>(e.sequence)}, {"published_sequence", static_cast<double>(get_panda_spi_error_sequence())},
    {"endpoint", e.endpoint}, {"attempt", static_cast<double>(e.attempt)}, {"result", e.result}, {"final_result", e.final_result},
    {"attempts", static_cast<double>(e.attempts)}, {"recoveries", static_cast<double>(e.recoveries)}, {"tx_len", e.tx_len},
    {"max_rx_len", e.max_rx_len}, {"timeout_ms", static_cast<double>(e.timeout_ms)}, {"phase", e.phase},
    {"lock_us", static_cast<double>(e.lock_us)}, {"turnaround_us", static_cast<double>(e.turnaround_us)},
    {"hack_us", static_cast<double>(e.hack_us)}, {"dack_us", static_cast<double>(e.dack_us)},
    {"recovery_us", static_cast<double>(e.recovery_us)}, {"total_us", static_cast<double>(e.total_us)},
    {"recovery_restarts", static_cast<double>(e.recovery_restarts)}};
}
int main() {
  unsetenv("SPI_ERR_PROB");
  fixture_active = true;
  std::string line;
  while (std::getline(std::cin, line)) {
    std::string error;
    scenario = Json::parse(line, error);
    if (!error.empty()) throw std::runtime_error(error);
    calls.clear(); logs.clear(); step_index = 0; defined_tx_bytes = 0; ignored_uninitialized_tx_bytes = 0;
    tick_ns = scenario["tick_ns"].is_null() ? 100000 : static_cast<uint64_t>(scenario["tick_ns"].number_value());
    Json::object output;
    Json::array outcomes;
    try {
      if (scenario["list"].bool_value()) {
        Json::array serials;
        for (auto &serial : PandaSpiHandle::list()) serials.emplace_back(serial);
        output["serials"] = serials;
      } else {
        PandaSpiHandle handle(scenario["serial"].string_value());
        output["serial"] = handle.hw_serial;
        for (const auto &op : scenario["operations"].array_items()) {
          const auto kind = op["kind"].string_value();
          const unsigned timeout = static_cast<unsigned>(op["timeout"].number_value());
          const size_t length = op["length"].int_value();
          if (length > 65535) throw std::runtime_error("fixture length outside supported domain");
          // Exact allocation lets ASan diagnose source writes beyond the caller's requested buffer.
          auto data = std::make_unique<uint8_t[]>(std::max<size_t>(1, length));
          std::fill_n(data.get(), std::max<size_t>(1, length), 0xa5);
          if (op["data"].array_items().size() > length) throw std::runtime_error("fixture input too long");
          for (size_t i = 0; i < op["data"].array_items().size(); ++i) data[i] = op["data"][i].int_value();
          int ret;
          if (kind == "control_read") ret = handle.control_read(op["request"].int_value(), op["param1"].int_value(), op["param2"].int_value(), data.get(), length, timeout);
          else if (kind == "control_write") ret = handle.control_write(op["request"].int_value(), op["param1"].int_value(), op["param2"].int_value(), timeout);
          else if (kind == "bulk_read") ret = handle.bulk_read(op["endpoint"].int_value(), data.get(), length, timeout);
          else if (kind == "bulk_write") ret = handle.bulk_write(op["endpoint"].int_value(), data.get(), length, timeout);
          else throw std::runtime_error("unknown SPI operation");
          outcomes.push_back(Json::object{{"return", ret}, {"data", bytes(data.get(), length)}, {"connected", handle.connected.load()}, {"healthy", handle.comms_healthy.load()}, {"event", event()}});
        }
      }
    } catch (const std::exception &failure) { output["error"] = failure.what(); }
    if (step_index != scenario["steps"].array_items().size()) throw std::runtime_error("unconsumed SPI fixture script");
    output["outcomes"] = outcomes; output["calls"] = calls; output["logs"] = logs;
    output["unspecified_tx_bytes"] = static_cast<double>(ignored_uninitialized_tx_bytes);
    output["clock_ns"] = static_cast<double>(timestamp); output["event"] = event();
    std::cout << Json(output).dump() << '\n';
  }
  fixture_active = false;
}

#include <cstdint>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
#include <json11/json11.hpp>
#include "common/util.h"

using json11::Json;
static Json step;
static Json::array commands;

struct Params {
  bool getBool(const std::string &key) {
    if (key != "IsDriverViewEnabled") throw std::runtime_error("unexpected Params read");
    const bool result = step["driver_view"].bool_value();
    commands.emplace_back(Json::object{{"driver_view", result}});
    return result;
  }
};

struct Camera {
  uint32_t getFrameId() const { return static_cast<uint32_t>(step["camera"]["frame_id"].number_value()); }
  int getIntegLines() const { return step["camera"]["integration_lines"].int_value(); }
};
struct Device {
  uint16_t getFanSpeedPercentDesired() const { return step["fan_speed"].int_value(); }
};
struct Event {
  Camera getDriverCameraState() const { return {}; }
  Device getDeviceState() const { return {}; }
  uint64_t getLogMonoTime() const { return static_cast<uint64_t>(step["camera"]["mono_time_ns"].number_value()); }
};
struct SubMaster {
  uint64_t frame = 0;
  explicit SubMaster(std::initializer_list<const char *>) {}
  void update(int) { frame = static_cast<uint64_t>(step["frame"].number_value()); }
  bool updated(const std::string &service) const {
    if (service == "deviceState") return !step["fan_speed"].is_null();
    if (service == "driverCameraState") return !step["camera"].is_null();
    throw std::runtime_error("unexpected service");
  }
  Event operator[](const std::string &) const { return {}; }
};
struct PubMaster {};
struct Panda {
  void set_fan_speed(uint16_t speed) { commands.emplace_back(Json::object{{"fan", speed}}); }
  void set_ir_pwr(uint16_t power) { commands.emplace_back(Json::object{{"panda_ir", power}}); }
};
struct Hardware {
  static void set_ir_power(int power) { commands.emplace_back(Json::object{{"hardware_ir", power}}); }
};
uint64_t nanos_since_boot() { return static_cast<uint64_t>(step["now_ns"].number_value()); }

#include "pandad_peripheral_body.inc"

int main() {
  std::string line;
  std::getline(std::cin, line);
  std::string error;
  const auto input = Json::parse(line, error);
  if (!error.empty()) throw std::runtime_error(error);
  Json::array trace;
  Panda panda;
  for (const auto &input_step : input.array_items()) {
    step = input_step;
    commands.clear();
    const bool no_fan_control = !step["fan_control"].is_null() && !step["fan_control"].bool_value();
    process_peripheral_state(&panda, nullptr, no_fan_control);
    trace.emplace_back(commands);
  }
  std::cout << Json(trace).dump() << std::endl;
}

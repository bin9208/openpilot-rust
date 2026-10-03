#include <algorithm>
#include <array>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <iostream>
#include <list>
#include <memory>
#include <new>
#include <sstream>
#include <stdexcept>
#include <vector>
#include <capnp/message.h>
#include <capnp/serialize.h>
#include <json11/json11.hpp>
#include "cereal/gen/cpp/car.capnp.h"
#include "cereal/gen/cpp/log.capnp.h"
#include "common/params.h"
#include "selfdrive/pandad/panda_comms.h"
#define private public
#define protected public
#include "selfdrive/pandad/pandad.h"
#undef protected
#undef private

using json11::Json;
static Json::array messages, commands;
static bool fill_allocation = false;
static uint8_t fill_byte = 0;
static unsigned filled_allocations = 0;

void *operator new(size_t size) {
  void *memory = std::malloc(size == 0 ? 1 : size);
  if (!memory) throw std::bad_alloc();
  if (fill_allocation && size == 512 * sizeof(capnp::word)) {
    std::memset(memory, fill_byte, size);
    ++filled_allocations;
  }
  return memory;
}
void operator delete(void *memory) noexcept { std::free(memory); }
void operator delete(void *memory, size_t) noexcept { std::free(memory); }
void *operator new[](size_t size) { return ::operator new(size); }
void operator delete[](void *memory) noexcept { std::free(memory); }
void operator delete[](void *memory, size_t) noexcept { std::free(memory); }

void cloudlog_e(int level, const char *, int, const char *, const char *format, ...) {
  std::array<char, 1024> text{};
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(text.data(), text.size(), format, arguments);
  va_end(arguments);
  messages.push_back(Json::object{{"level", level}, {"message", text.data()}});
}

class FixtureHandle final : public PandaCommsHandle {
public:
  size_t index;
  explicit FixtureHandle(size_t value) : index(value) {}
  void cleanup() override {}
  int control_write(uint8_t request, uint16_t value, uint16_t parameter, unsigned int) override {
    commands.push_back(Json::object{{"panda", static_cast<int>(index)}, {"request", request}, {"value", value}, {"index", parameter}});
    return 0;
  }
  int control_read(uint8_t, uint16_t, uint16_t, unsigned char *, uint16_t, unsigned int) override {
    throw std::runtime_error("unexpected control read");
  }
  int bulk_write(unsigned char, unsigned char *, int, unsigned int) override {
    throw std::runtime_error("unexpected bulk write");
  }
  int bulk_read(unsigned char, unsigned char *, int, unsigned int) override {
    throw std::runtime_error("unexpected bulk read");
  }
};

Json bytes(const uint8_t *begin, size_t length) {
  Json::array result;
  for (size_t index = 0; index < length; ++index) result.emplace_back(begin[index]);
  return result;
}

Json car_params(const Json &request) {
  capnp::MallocMessageBuilder message;
  auto cp = message.initRoot<cereal::CarParams>();
  cp.setAlternativeExperience(request["alternative"].int_value());
  auto configs = cp.initSafetyConfigs(request["configs"].array_items().size());
  size_t index = 0;
  for (const auto &config : request["configs"].array_items()) {
    configs[index].setSafetyModel(static_cast<cereal::CarParams::SafetyModel>(config[0].int_value()));
    configs[index++].setSafetyParam(config[1].int_value());
  }
  auto wire = capnp::messageToFlatArray(message);
  return bytes(wire.asBytes().begin(), wire.asBytes().size());
}

Json safety(const Json &request) {
  setenv("PARAMS_ROOT", request["params_root"].string_value().c_str(), 1);
  setenv("OPENPILOT_PREFIX", "panda-safety-fixture", 1);
  Params params;
  for (const auto key : {"ObdMultiplexingEnabled", "ObdMultiplexingChanged", "FirmwareQueryDone", "ControlsReady", "CarParams"}) params.remove(key);
  std::vector<std::unique_ptr<Panda>> owners;
  std::vector<Panda *> pandas;
  for (int index = 0; index < request["pandas"].int_value(); ++index) {
    auto panda = std::make_unique<Panda>(static_cast<uint32_t>(index * 4));
    panda->handle = std::make_unique<FixtureHandle>(index);
    pandas.push_back(panda.get());
    owners.push_back(std::move(panda));
  }
  PandaSafety controller(pandas);
  Json::array results;
  for (const auto &operation : request["operations"].array_items()) {
    for (const auto &[key, value] : operation["params"].object_items()) {
      if (value.is_null()) params.remove(key);
      else {
        std::string raw;
        for (const auto &byte : value.array_items()) raw.push_back(byte.int_value());
        params.put(key, raw);
      }
    }
    commands.clear();
    messages.clear();
    bool failed = false;
    fill_allocation = !operation["allocation_fill"].is_null();
    fill_byte = operation["allocation_fill"].int_value();
    filled_allocations = 0;
    try { controller.configureSafetyMode(operation["onroad"].bool_value()); }
    catch (const kj::Exception &) { failed = true; }
    fill_allocation = false;
    auto changed = params.get("ObdMultiplexingChanged");
    Json::object row{{"failed", failed}, {"commands", commands}, {"logs", messages},
      {"changed", bytes(reinterpret_cast<const uint8_t *>(changed.data()), changed.size())},
      {"state", Json::object{{"initialized", controller.initialized_}, {"log_once", controller.log_once_},
        {"safety_configured", controller.safety_configured_}, {"prev_obd_multiplexing", controller.prev_obd_multiplexing_}}}};
    if (!operation["allocation_fill"].is_null()) row["filled_allocations"] = static_cast<int>(filled_allocations);
    results.push_back(row);
  }
  return Json::object{{"results", results}};
}

int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    std::string error;
    auto request = Json::parse(line, error);
    if (!error.empty()) throw std::runtime_error(error);
    auto result = request["op"].string_value() == "car_params" ? car_params(request) : safety(request);
    std::cout << result.dump() << '\n';
  }
}

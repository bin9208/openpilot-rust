#include <algorithm>
#include <array>
#include <bitset>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <memory>
#include <optional>
#include <stdexcept>
#include <unordered_map>
#include <vector>
#include <json11/json11.hpp>
#include "cereal/messaging/messaging.h"
#include "common/util.h"
#include "common/swaglog.h"
#include "selfdrive/pandad/panda_comms.h"
#define protected public
#define private public
#include "selfdrive/pandad/panda.h"
#undef private
#undef protected

using json11::Json;
static Json step;
static Json::array actions;
static uint64_t timestamp;
ExitHandler do_exit;

extern "C" int __wrap_clock_gettime(clockid_t, struct timespec *out) {
  out->tv_sec = timestamp / 1000000000;
  out->tv_nsec = timestamp % 1000000000;
  return 0;
}

void cloudlog_e(int level, const char *, int, const char *, const char *format, ...) {
  std::array<char, 2048> text{};
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(text.data(), text.size(), format, arguments);
  va_end(arguments);
  actions.emplace_back(Json::array{"log", level, text.data()});
}

std::string text_bytes(const Json &bytes) {
  std::string text;
  for (const auto &byte : bytes.array_items()) text.push_back(static_cast<char>(byte.int_value()));
  return text;
}

class RecordedHandle : public PandaCommsHandle {
public:
  size_t index;
  explicit RecordedHandle(size_t index) : index(index) {}
  void cleanup() override {}
  int control_write(uint8_t request, uint16_t value, uint16_t parameter, unsigned timeout) override {
    actions.emplace_back(Json::array{"write", int(index), request, value, parameter, double(timeout)});
    return 0;
  }
  int control_read(uint8_t request, uint16_t value, uint16_t parameter, unsigned char *output, uint16_t length, unsigned timeout) override {
    actions.emplace_back(Json::array{"read", int(index), request, value, parameter, length, double(timeout)});
    const auto &panda = step["pandas"][index];
    const Json &data = request == 0xd2 ? panda["health"] : panda["can"][value];
    if (request != 0xd2 && request != 0xc2) throw std::runtime_error("unexpected control read");
    if (data.is_null()) return -4;
    if (data.array_items().size() != length) throw std::runtime_error("invalid fixture packet length");
    size_t i = 0;
    for (const auto &byte : data.array_items()) output[i++] = byte.int_value();
    return length;
  }
  int bulk_write(unsigned char, unsigned char *, int, unsigned) override { throw std::runtime_error("unexpected bulk write"); }
  int bulk_read(unsigned char, unsigned char *, int, unsigned) override { throw std::runtime_error("unexpected bulk read"); }
};

std::vector<std::string> PandaUsbHandle::list() {
  actions.emplace_back(Json::array{"list"});
  std::vector<std::string> serials;
  for (const auto &serial : step["listed"].array_items()) serials.push_back(text_bytes(serial));
  return serials;
}
std::vector<std::string> PandaSpiHandle::list() { throw std::runtime_error("unexpected SPI enumeration"); }
PubMaster::PubMaster(const std::vector<const char *> &) {}
PubMaster::~PubMaster() {}
int PubMaster::send(const char *name, MessageBuilder &message) {
  Json::array bytes;
  for (const auto byte : message.toBytes()) bytes.emplace_back(byte);
  actions.emplace_back(Json::array{"publish", name, bytes});
  return 0;
}

#include "pandad_state_body.inc"

int main() {
  std::string line, error;
  std::getline(std::cin, line);
  const auto input = Json::parse(line, error);
  if (!error.empty()) throw std::runtime_error(error);
  std::vector<std::unique_ptr<Panda>> owned;
  std::vector<Panda *> pandas;
  for (const auto &identity : input["identities"].array_items()) {
    auto panda = std::unique_ptr<Panda>(new Panda(pandas.size() * 4));
    panda->hw_type = static_cast<cereal::PandaState::PandaType>(identity["hardware_type"].int_value());
    auto handle = std::make_unique<RecordedHandle>(pandas.size());
    handle->hw_serial = text_bytes(identity["serial"]);
    panda->handle = std::move(handle);
    pandas.push_back(panda.get());
    owned.push_back(std::move(panda));
  }
  PubMaster publisher({"pandaStates"});
  Json::array results;
  for (const auto &input_step : input["steps"].array_items()) {
    step = input_step;
    timestamp = static_cast<uint64_t>(step["now_ns"].number_value());
    actions.clear();
    for (size_t i = 0; i < pandas.size(); ++i) pandas[i]->handle->comms_healthy = step["pandas"][i]["healthy"].bool_value();
    const auto &state = step["input"];
    const auto ignition = process_panda_state(pandas, &publisher, state["engaged"].bool_value(), state["onroad"].bool_value(),
                                               state["spoofing_started"].bool_value());
    results.emplace_back(Json::object{{"ignition", ignition ? Json(*ignition) : Json()}, {"exit", bool(do_exit)}, {"actions", actions}});
  }
  std::cout << Json(results).dump() << std::endl;
}

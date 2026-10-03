#include <algorithm>
#include <array>
#include <cstdarg>
#include <cstdio>
#include <deque>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <vector>
#include <capnp/message.h>
#include <json11/json11.hpp>
#include "cereal/gen/cpp/car.capnp.h"
#include "cereal/gen/cpp/log.capnp.h"
#include "selfdrive/pandad/panda_comms.h"
#define private public
#define protected public
#include "selfdrive/pandad/panda.h"
#include "selfdrive/pandad/spi_alert.h"
#undef protected
#undef private

using json11::Json;
static Json::array log_messages;

void cloudlog_e(int level, const char *, int, const char *, const char *format, ...) {
  std::array<char, 1024> text{};
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(text.data(), text.size(), format, arguments);
  va_end(arguments);
  log_messages.push_back(Json::object{{"level", level}, {"message", text.data()}});
}

class FixtureHandle final : public PandaCommsHandle {
public:
  unsigned resets = 0;
  void cleanup() override {}
  int control_write(uint8_t request, uint16_t a, uint16_t b, unsigned int) override {
    if (request != 0xc0 || a != 0 || b != 0) throw std::runtime_error("unexpected control write");
    ++resets;
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

Json protocol(const Json &request) {
  Panda panda(static_cast<uint32_t>(request["offset"].number_value()));
  auto handle = std::make_unique<FixtureHandle>();
  FixtureHandle *observed = handle.get();
  panda.handle = std::move(handle);
  std::fill(std::begin(panda.receive_buffer), std::end(panda.receive_buffer), 0);
  log_messages.clear();
  if (request["op"].string_value() == "pack") {
    capnp::MallocMessageBuilder builder;
    auto frames = builder.initRoot<cereal::Event>().initSendcan(request["frames"].array_items().size());
    unsigned index = 0;
    for (const auto &frame : request["frames"].array_items()) {
      frames[index].setAddress(static_cast<uint32_t>(frame["address"].number_value()));
      frames[index].setSrc(frame["src"].int_value());
      std::vector<uint8_t> data;
      for (const auto &value : frame["data"].array_items()) data.push_back(value.int_value());
      frames[index++].setDat(kj::arrayPtr(data.data(), data.size()));
    }
    Json::array chunks;
    panda.pack_can_buffer(frames.asReader(), [&](uint8_t *data, size_t size) { chunks.push_back(bytes(data, size)); });
    return Json::object{{"chunks", chunks}};
  }
  std::vector<can_frame> frames;
  Json::array results;
  for (const auto &chunk : request["chunks"].array_items()) {
    if (panda.receive_buffer_size + chunk.array_items().size() > sizeof(panda.receive_buffer)) {
      throw std::runtime_error("fixture receive buffer overflow");
    }
    for (const auto &value : chunk.array_items()) panda.receive_buffer[panda.receive_buffer_size++] = value.int_value();
    bool ok = panda.unpack_can_buffer(panda.receive_buffer, panda.receive_buffer_size, frames);
    Json::array output;
    for (const auto &frame : frames) {
      output.push_back(Json::object{{"address", static_cast<double>(frame.address)}, {"src", static_cast<double>(frame.src)},
        {"data", bytes(reinterpret_cast<const uint8_t *>(frame.dat.data()), frame.dat.size())}});
    }
    results.push_back(Json::object{{"ok", ok}, {"frames", output}, {"resets", static_cast<int>(observed->resets)},
      {"remaining", bytes(panda.receive_buffer, panda.receive_buffer_size)}, {"logs", log_messages}});
  }
  return Json::object{{"results", results}};
}

Json alerts(const Json &request) {
  PandaSpiAlertTracker tracker;
  Json::array results;
  for (const auto &operation : request["operations"].array_items()) {
    auto now = std::stoull(operation["now"].string_value());
    auto name = operation["op"].string_value();
    Json value;
    if (name == "onroad") tracker.update_onroad(operation["value"].bool_value(), now);
    else if (name == "observe") value = tracker.observe(now, std::stoull(operation["count"].string_value()), operation["terminal"].bool_value());
    else if (name == "ready") value = tracker.ready(now);
    else if (name == "mark") tracker.mark_capture_requested();
    else throw std::runtime_error("unknown alert operation");
    Json::array times;
    for (auto time : tracker.recovered_event_times_ms_) times.emplace_back(std::to_string(time));
    results.push_back(Json::object{{"result", value}, {"onroad", tracker.is_onroad_}, {"since", std::to_string(tracker.onroad_since_ms_)},
      {"pending", tracker.pending_}, {"pending_since", std::to_string(tracker.pending_since_ms_)},
      {"captured", tracker.capture_requested_}, {"times", times}});
  }
  return Json::object{{"results", results}};
}

int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    std::string error;
    auto request = Json::parse(line, error);
    if (!error.empty()) throw std::runtime_error(error);
    auto result = request["op"].string_value() == "alerts" ? alerts(request) : protocol(request);
    std::cout << result.dump() << '\n';
  }
}

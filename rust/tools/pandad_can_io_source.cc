#include <algorithm>
#include <array>
#include <cstdarg>
#include <cstdio>
#include <deque>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <vector>
#include <capnp/message.h>
#include <json11/json11.hpp>
#include "cereal/gen/cpp/log.capnp.h"
#include "selfdrive/pandad/panda_comms.h"
#define private public
#define protected public
#include "selfdrive/pandad/panda.h"
#undef protected
#undef private

using json11::Json;
static Json::array calls;

void cloudlog_e(int level, const char *, int, const char *, const char *format, ...) {
  std::array<char, 1024> text{};
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(text.data(), text.size(), format, arguments);
  va_end(arguments);
  calls.emplace_back(Json::object{{"op", "log"}, {"level", level}, {"message", text.data()}});
}

Json bytes(const uint8_t *begin, size_t length) {
  Json::array result;
  for (size_t i = 0; i < length; ++i) result.emplace_back(begin[i]);
  return result;
}

class RecordedHandle final : public PandaCommsHandle {
public:
  std::deque<Json> replies;
  int write_result = 0;
  void cleanup() override {}
  int control_write(uint8_t request, uint16_t value, uint16_t index, unsigned timeout) override {
    calls.emplace_back(Json::object{{"op", "control"}, {"request", request}, {"value", value}, {"index", index},
                                   {"timeout", double(timeout)}});
    return 0;
  }
  int control_read(uint8_t, uint16_t, uint16_t, unsigned char *, uint16_t, unsigned) override {
    throw std::runtime_error("unexpected control read");
  }
  int bulk_write(unsigned char endpoint, unsigned char *data, int length, unsigned timeout) override {
    calls.emplace_back(Json::object{{"op", "write"}, {"endpoint", endpoint}, {"data", bytes(data, length)},
                                   {"timeout", double(timeout)}});
    return write_result;
  }
  int bulk_read(unsigned char endpoint, unsigned char *data, int length, unsigned timeout) override {
    calls.emplace_back(Json::object{{"op", "read"}, {"endpoint", endpoint}, {"length", length}, {"timeout", double(timeout)}});
    if (endpoint == 0xab) return 0;
    if (replies.empty()) throw std::runtime_error("unexpected bulk read");
    const auto reply = replies.front();
    replies.pop_front();
    if (length < 0 || reply["data"].array_items().size() > static_cast<size_t>(length)) {
      throw std::runtime_error("receive fixture overflow");
    }
    size_t i = 0;
    for (const auto &byte : reply["data"].array_items()) data[i++] = byte.int_value();
    comms_healthy = reply["healthy"].bool_value();
    return reply["count"].int_value();
  }
};

Json run(const Json &input) {
  Panda panda(static_cast<uint32_t>(input["offset"].number_value()));
  auto handle = std::make_unique<RecordedHandle>();
  auto *record = handle.get();
  for (const auto &reply : input["replies"].array_items()) record->replies.push_back(reply);
  record->write_result = input["write_result"].int_value();
  panda.handle = std::move(handle);
  std::fill(std::begin(panda.receive_buffer), std::end(panda.receive_buffer), 0);
  calls.clear();
  std::vector<can_frame> received;
  Json::array results;
  for (const auto &operation : input["operations"].array_items()) {
    Json value;
    const auto name = operation["op"].string_value();
    if (name == "send") {
      capnp::MallocMessageBuilder builder;
      auto frames = builder.initRoot<cereal::Event>().initSendcan(operation["frames"].array_items().size());
      unsigned i = 0;
      for (const auto &frame : operation["frames"].array_items()) {
        frames[i].setAddress(static_cast<uint32_t>(frame["address"].number_value()));
        frames[i].setSrc(frame["src"].int_value());
        std::vector<uint8_t> data;
        for (const auto &byte : frame["data"].array_items()) data.push_back(byte.int_value());
        frames[i++].setDat(kj::arrayPtr(data.data(), data.size()));
      }
      panda.can_send(frames.asReader());
    } else if (name == "receive") {
      value = panda.can_receive(received);
    } else throw std::runtime_error("unknown operation");
    Json::array frames;
    for (const auto &frame : received) {
      frames.emplace_back(Json::object{{"address", double(frame.address)}, {"src", double(frame.src)},
        {"data", bytes(reinterpret_cast<const uint8_t *>(frame.dat.data()), frame.dat.size())}});
    }
    results.emplace_back(Json::object{{"value", value}, {"frames", frames}, {"calls", calls},
                                     {"remaining", bytes(panda.receive_buffer, panda.receive_buffer_size)}});
  }
  return Json::object{{"results", results}, {"unused_replies", static_cast<int>(record->replies.size())}};
}

int main() {
  for (std::string line; std::getline(std::cin, line);) {
    std::string error;
    const auto input = Json::parse(line, error);
    if (!error.empty()) throw std::runtime_error(error);
    std::cout << run(input).dump() << std::endl;
  }
}

#include <array>
#include <cstring>
#include <deque>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <vector>
#include <json11/json11.hpp>
#include "selfdrive/pandad/panda_comms.h"
#define private public
#define protected public
#include "selfdrive/pandad/panda.h"
#undef protected
#undef private

using json11::Json;
void cloudlog_e(int, const char *, int, const char *, const char *, ...) {}

Json bytes(const uint8_t *begin, size_t length) {
  Json::array output;
  for (size_t i = 0; i < length; ++i) output.emplace_back(begin[i]);
  return output;
}

class RecordedHandle : public PandaCommsHandle {
public:
  std::deque<Json> replies;
  Json::array calls;
  void cleanup() override {}
  int control_write(uint8_t request, uint16_t value, uint16_t index, unsigned timeout) override {
    calls.emplace_back(Json::object{{"read", false}, {"request", request}, {"value", value}, {"index", index},
                                    {"timeout_ms", static_cast<double>(timeout)}, {"length", 0}});
    return 0;
  }
  int control_read(uint8_t request, uint16_t value, uint16_t index, unsigned char *output, uint16_t length, unsigned timeout) override {
    calls.emplace_back(Json::object{{"read", true}, {"request", request}, {"value", value}, {"index", index},
                                    {"timeout_ms", static_cast<double>(timeout)}, {"length", length}});
    if (replies.empty()) throw std::runtime_error("unexpected control read");
    const auto reply = replies.front();
    replies.pop_front();
    if (reply["request"].int_value() != request || reply["bytes"].array_items().size() > length) {
      throw std::runtime_error("control read contract mismatch");
    }
    size_t i = 0;
    for (const auto &byte : reply["bytes"].array_items()) output[i++] = byte.int_value();
    return reply["count"].int_value();
  }
  int bulk_write(unsigned char, unsigned char *, int, unsigned) override { throw std::runtime_error("unexpected bulk write"); }
  int bulk_read(unsigned char, unsigned char *, int, unsigned) override { throw std::runtime_error("unexpected bulk read"); }
};

Json health(const health_t &h) {
  uint32_t bits;
  std::memcpy(&bits, &h.interrupt_load_pkt, sizeof(bits));
  Json::object fields{
    {"uptime", double(h.uptime_pkt)}, {"voltage", double(h.voltage_pkt)}, {"current", double(h.current_pkt)},
    {"safety_tx_blocked", double(h.safety_tx_blocked_pkt)}, {"safety_rx_invalid", double(h.safety_rx_invalid_pkt)},
    {"tx_overflow", double(h.tx_buffer_overflow_pkt)}, {"rx_overflow", double(h.rx_buffer_overflow_pkt)},
    {"faults", double(h.faults_pkt)}, {"ignition_line", h.ignition_line_pkt}, {"ignition_can", h.ignition_can_pkt},
    {"controls_allowed", h.controls_allowed_pkt}, {"harness_status", h.car_harness_status_pkt},
    {"safety_model", h.safety_mode_pkt}, {"safety_param", h.safety_param_pkt}, {"fault_status", h.fault_status_pkt},
    {"power_save", h.power_save_enabled_pkt}, {"heartbeat_lost", h.heartbeat_lost_pkt},
    {"alternative_experience", h.alternative_experience_pkt}, {"interrupt_load", h.interrupt_load_pkt},
    {"fan_power", h.fan_power}, {"safety_rx_checks_invalid", h.safety_rx_checks_invalid_pkt},
    {"spi_checksum_errors", h.spi_checksum_error_count_pkt}, {"fan_stall_count", h.fan_stall_count},
    {"sbu1_mv", h.sbu1_voltage_mV}, {"sbu2_mv", h.sbu2_voltage_mV}, {"som_reset_triggered", h.som_reset_triggered}
  };
  return Json::object{{"health", fields}, {"interrupt_bits", double(bits)}};
}

Json can_health(const can_health_t &h) {
  return Json::object{
    {"bus_off", h.bus_off}, {"bus_off_count", double(h.bus_off_cnt)}, {"error_warning", h.error_warning},
    {"error_passive", h.error_passive}, {"last_error", h.last_error}, {"last_stored_error", h.last_stored_error},
    {"last_data_error", h.last_data_error}, {"last_data_stored_error", h.last_data_stored_error},
    {"receive_error_count", h.receive_error_cnt}, {"transmit_error_count", h.transmit_error_cnt},
    {"total_errors", double(h.total_error_cnt)}, {"total_tx_lost", double(h.total_tx_lost_cnt)},
    {"total_rx_lost", double(h.total_rx_lost_cnt)}, {"total_tx", double(h.total_tx_cnt)},
    {"total_rx", double(h.total_rx_cnt)}, {"total_forwarded", double(h.total_fwd_cnt)},
    {"total_tx_checksum_errors", double(h.total_tx_checksum_error_cnt)}, {"can_speed", h.can_speed},
    {"can_data_speed", h.can_data_speed}, {"canfd_enabled", h.canfd_enabled}, {"brs_enabled", h.brs_enabled},
    {"canfd_non_iso", h.canfd_non_iso}, {"irq0_rate", double(h.irq0_call_rate)}, {"irq1_rate", double(h.irq1_call_rate)},
    {"irq2_rate", double(h.irq2_call_rate)}, {"core_reset_count", double(h.can_core_reset_cnt)}
  };
}

Json run(const Json &input) {
  Panda panda(4);
  auto handle = std::make_unique<RecordedHandle>();
  auto *record = handle.get();
  for (const auto &reply : input["replies"].array_items()) record->replies.push_back(reply);
  panda.handle = std::move(handle);
  panda.hw_type = panda.get_hw_type();
  panda.can_reset_communications();
  Json::array results;
  for (const auto &operation : input["operations"].array_items()) {
    const auto op = operation["op"].string_value();
    if (op == "health") {
      const auto value = panda.get_state(); results.emplace_back(value ? health(*value) : Json());
    } else if (op == "can_health") {
      const auto value = panda.get_can_state(operation["bus"].int_value()); results.emplace_back(value ? can_health(*value) : Json());
    } else if (op == "fan_speed") {
      results.emplace_back(panda.get_fan_speed());
    } else if (op == "signature") {
      const auto value = panda.get_firmware_version(); results.emplace_back(value ? bytes(value->data(), value->size()) : Json());
    } else if (op == "serial") {
      const auto value = panda.get_serial(); results.emplace_back(value ? bytes(reinterpret_cast<const uint8_t *>(value->data()), value->size()) : Json());
    } else if (op == "serial_read") {
      const auto value = panda.serial_read(operation["port"].int_value()); results.emplace_back(bytes(reinterpret_cast<const uint8_t *>(value.data()), value.size()));
    } else throw std::runtime_error("unknown operation");
  }
  return Json::object{{"hardware_type", static_cast<int>(panda.hw_type)}, {"remaining", static_cast<int>(record->replies.size())},
                      {"results", results}, {"calls", record->calls}};
}

int main() {
  for (std::string line; std::getline(std::cin, line);) {
    std::string error;
    const auto input = Json::parse(line, error);
    if (!error.empty()) throw std::runtime_error(error);
    std::cout << run(input).dump() << std::endl;
  }
}

#pragma once
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace sensord_kernel {
std::uint8_t read_byte(int fd, std::uint16_t address, std::uint8_t reg, bool force);
void write_byte(int fd, std::uint16_t address, std::uint8_t reg, std::uint8_t value, bool force);
std::unique_ptr<std::vector<std::uint8_t>> read_block(int fd, std::uint16_t address, std::uint8_t reg, std::size_t length, bool force);
void realtime(bool pc);
class Gpio {
 public:
  explicit Gpio(int fd) : fd_(fd) {}
  ~Gpio();
  Gpio(const Gpio&) = delete;
  Gpio& operator=(const Gpio&) = delete;
  int poll_event(int timeout_ms);
  std::unique_ptr<std::vector<std::uint8_t>> read_events();
 private:
  int fd_;
};
std::unique_ptr<Gpio> open_gpio(const std::string& path, const std::string& label, std::uint32_t pin);
}

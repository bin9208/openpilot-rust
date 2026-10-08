#pragma once
#include <cstdint>
#include <memory>
#include <string>
#include "rust/cxx.h"
namespace panda_spi_kernel {
struct Call;
struct OptionCall;
class Handle {
public:
  explicit Handle(const std::string &path) : path_(path) {}
  ~Handle();
  bool exists() const;
  int open();
  Call open_call();
  OptionCall read_option(uint8_t option);
  Call configure(uint8_t option, uint32_t value);
  Call transfer(rust::Slice<const uint8_t> tx, rust::Slice<uint8_t> rx);
  Call transfer_at_speed(rust::Slice<const uint8_t> tx, rust::Slice<uint8_t> rx, uint32_t speed, uint8_t bits);
  Call read_bytes(rust::Slice<uint8_t> rx);
  Call write_bytes(rust::Slice<const uint8_t> tx);
  Call firmware_transfer(uint8_t endpoint, rust::Slice<const uint8_t> tx, rust::Slice<uint8_t> rx, bool disconnect);
  Call flock_call(bool exclusive);
  void flock(bool exclusive);
  void close();
private:
  std::string path_;
  int fd_ = -1;
};
std::unique_ptr<Handle> create(const std::string &path);
uint64_t now_ns();
void yield_now();
void sleep_us(uint32_t micros);
Call set_scheduler(int32_t policy, int32_t priority);
std::unique_ptr<std::string> errno_description(int error);
double parse_probability(const std::string &value);
uint32_t random();
void diagnostic_print(rust::Str text);
}

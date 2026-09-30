// Linux UAPI boundary for common/i2c.py and common/gpio.py; sensor policy stays in Rust.
#include "kernel.h"
#include <algorithm>
#include <array>
#include <cerrno>
#include <chrono>
#include <cstring>
#include <fcntl.h>
#include <linux/gpio.h>
#include <linux/i2c-dev.h>
#include <linux/i2c.h>
#include <poll.h>
#include <sched.h>
#include <stdexcept>
#include <system_error>
#include <sys/ioctl.h>
#include <unistd.h>

namespace sensord_kernel {
namespace {
void checked(int result) {
  if (result < 0) throw std::system_error(errno, std::generic_category());
}
void address(int fd, std::uint16_t address, bool force) {
  checked(ioctl(fd, force ? I2C_SLAVE_FORCE : I2C_SLAVE, static_cast<unsigned long>(address)));
}
void access(int fd, std::uint8_t direction, std::uint8_t reg, std::uint32_t size, i2c_smbus_data& data) {
  i2c_smbus_ioctl_data request;
  std::memset(&request, 0, sizeof(request));
  request.read_write = direction;
  request.command = reg;
  request.size = size;
  request.data = &data;
  checked(ioctl(fd, I2C_SMBUS, &request));
}
}
std::uint8_t read_byte(int fd, std::uint16_t addr, std::uint8_t reg, bool force) {
  address(fd, addr, force);
  i2c_smbus_data data;
  std::memset(&data, 0, sizeof(data));
  access(fd, I2C_SMBUS_READ, reg, I2C_SMBUS_BYTE_DATA, data);
  return data.byte;
}
void write_byte(int fd, std::uint16_t addr, std::uint8_t reg, std::uint8_t value, bool force) {
  address(fd, addr, force);
  i2c_smbus_data data;
  std::memset(&data, 0, sizeof(data));
  data.byte = value;
  access(fd, I2C_SMBUS_WRITE, reg, I2C_SMBUS_BYTE_DATA, data);
}
std::unique_ptr<std::vector<std::uint8_t>> read_block(int fd, std::uint16_t addr, std::uint8_t reg, std::size_t length, bool force) {
  address(fd, addr, force);
  if (length > I2C_SMBUS_BLOCK_MAX) throw std::invalid_argument("length must be 0..32");
  i2c_smbus_data data;
  std::memset(&data, 0, sizeof(data));
  data.block[0] = static_cast<std::uint8_t>(length);
  access(fd, I2C_SMBUS_READ, reg, I2C_SMBUS_I2C_BLOCK_DATA, data);
  const std::size_t count = std::min<std::size_t>(data.block[0] ? data.block[0] : length, length);
  return std::make_unique<std::vector<std::uint8_t>>(data.block + 1, data.block + 1 + count);
}
void realtime(bool pc) {
  if (pc) return;
  sched_param priority{};
  priority.sched_priority = 1;
  checked(sched_setscheduler(0, SCHED_FIFO, &priority));
  cpu_set_t cpus;
  CPU_ZERO(&cpus);
  CPU_SET(1, &cpus);
  checked(sched_setaffinity(0, sizeof(cpus), &cpus));
}
Gpio::~Gpio() { if (fd_ >= 0) close(fd_); }
int Gpio::poll_event(int timeout_ms) {
  const auto end = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
  for (;;) {
    pollfd descriptor{fd_, POLLIN | POLLPRI, 0};
    const int result = poll(&descriptor, 1, timeout_ms);
    if (result >= 0) return result ? descriptor.revents : 0;
    if (errno != EINTR) checked(result);
    const auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(end - std::chrono::steady_clock::now()).count();
    if (remaining <= 0) return 0;
    timeout_ms = static_cast<int>(remaining);
  }
}
std::unique_ptr<std::vector<std::uint8_t>> Gpio::read_events() {
  static_assert(sizeof(gpioevent_data) == 16);
  std::array<std::uint8_t, sizeof(gpioevent_data) * 16> bytes{};
  ssize_t count;
  do { count = read(fd_, bytes.data(), bytes.size()); } while (count < 0 && errno == EINTR);
  if (count < 0) checked(-1);
  return std::make_unique<std::vector<std::uint8_t>>(bytes.begin(), bytes.begin() + count);
}
std::unique_ptr<Gpio> open_gpio(const std::string& path, const std::string& label, std::uint32_t pin) {
  gpioevent_request request;
  std::memset(&request, 0, sizeof(request));
  request.lineoffset = pin;
  request.handleflags = GPIOHANDLE_REQUEST_INPUT;
  request.eventflags = GPIOEVENT_REQUEST_BOTH_EDGES;
  std::memcpy(request.consumer_label, label.data(), std::min<std::size_t>(label.size(), 31));
  int fd;
  do { fd = open(path.c_str(), O_RDONLY | O_CLOEXEC); } while (fd < 0 && errno == EINTR);
  checked(fd);
  const int status = ioctl(fd, GPIO_GET_LINEEVENT_IOCTL, &request);
  const int error = errno;
  const int closed = close(fd);
  if (status < 0) throw std::system_error(error, std::generic_category());
  if (closed < 0) { const int close_error = errno; close(request.fd); throw std::system_error(close_error, std::generic_category()); }
  if (request.fd < 0) throw std::runtime_error("invalid GPIO event descriptor");
  return std::make_unique<Gpio>(request.fd);
}
}

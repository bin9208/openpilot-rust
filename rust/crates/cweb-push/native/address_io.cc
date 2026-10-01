#include "native/address_io.h"
#include <algorithm>
#include <arpa/inet.h>
#include <cerrno>
#include <cstring>
#include <net/if.h>
#include <stdexcept>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <unistd.h>

namespace openpilot::cweb {
std::string interface_address(const std::uint8_t *name, std::size_t length) {
  int fd = socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC, 0);
  if (fd < 0) throw std::runtime_error(std::strerror(errno));
  struct ifreq request = {};
  const auto count = std::min(length, sizeof(request.ifr_name) - 1);
  if (count) std::memcpy(request.ifr_name, name, count);
  int result = ioctl(fd, SIOCGIFADDR, &request);
  int error = errno;
  close(fd);
  if (result < 0) throw std::runtime_error(std::strerror(error));
  char address[INET_ADDRSTRLEN];
  struct in_addr value;
  std::memcpy(&value, request.ifr_addr.sa_data + 2, sizeof(value));
  if (!inet_ntop(AF_INET, &value, address, sizeof(address))) throw std::runtime_error(std::strerror(errno));
  return address;
}
}

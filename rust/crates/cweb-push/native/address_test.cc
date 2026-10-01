#include "native/address_io.h"
#include <arpa/inet.h>
#include <cassert>
#include <cerrno>
#include <cstdarg>
#include <cstring>
#include <net/if.h>
#include <stdexcept>
#include <string>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <vector>

static int mode = 0;
static std::vector<int> operations;
static std::string last_name;

extern "C" int fixture_socket(int domain, int type, int protocol) noexcept {
  assert(domain == AF_INET && type == (SOCK_DGRAM | SOCK_CLOEXEC) && protocol == 0);
  operations.push_back(1);
  if (mode == 1) { errno = EMFILE; return -1; }
  return 41;
}
extern "C" int fixture_ioctl(int fd, unsigned long command, ...) noexcept {
  assert(fd == 41 && command == SIOCGIFADDR);
  operations.push_back(2);
  va_list values;
  va_start(values, command);
  auto *request = va_arg(values, struct ifreq *);
  va_end(values);
  last_name.assign(request->ifr_name, strnlen(request->ifr_name, sizeof(request->ifr_name)));
  if (mode == 2) { errno = ENODEV; return -1; }
  request->ifr_addr.sa_family = AF_INET;
  struct in_addr address;
  assert(inet_pton(AF_INET, "192.0.2.7", &address) == 1);
  std::memcpy(request->ifr_addr.sa_data + 2, &address, sizeof(address));
  return 0;
}
extern "C" int fixture_close(int fd) {
  assert(fd == 41);
  operations.push_back(3);
  return 0;
}

int main() {
  for (const std::string name : {"", "wlan0", "long-interface-name-xyz"}) {
    operations.clear();
    auto result = openpilot::cweb::interface_address(reinterpret_cast<const std::uint8_t *>(name.data()), name.size());
    assert(result == "192.0.2.7");
    assert(last_name == name.substr(0, 15));
    assert((operations == std::vector<int>{1, 2, 3}));
  }
  for (int failure : {1, 2}) {
    mode = failure;
    operations.clear();
    bool failed = false;
    try { openpilot::cweb::interface_address(nullptr, 0); }
    catch (const std::runtime_error &) { failed = true; }
    assert(failed);
    assert(operations == (failure == 1 ? std::vector<int>{1} : std::vector<int>{1, 2, 3}));
  }
}

#include "serial.h"
#include <cassert>
#include <cerrno>
#include <cstdarg>
#include <cstdio>
#include <system_error>
#include <vector>
#include <sys/ioctl.h>

static std::vector<int> observed;
static int injected_error = 0;
extern "C" int ioctl(int fd, unsigned long request, ...) noexcept {
  assert(fd == 42 && request == TIOCMBIS);
  va_list arguments;
  va_start(arguments, request);
  int *value = va_arg(arguments, int *);
  observed.push_back(*value);
  va_end(arguments);
  if (injected_error != 0) {
    errno = injected_error;
    return -1;
  }
  return 0;
}
int main() {
  for (int fault : {0, EINVAL, ENOTTY, EIO}) {
    observed.clear();
    injected_error = fault;
    bool failed = false;
    try { ublox_serial::raise_modem_lines(42); }
    catch (const std::system_error &error) {
      assert(error.code().value() == EIO);
      failed = true;
    }
    assert(failed == (fault == EIO));
    if (fault == 0) assert((observed == std::vector<int>{TIOCM_DTR, TIOCM_RTS}));
    else assert((observed == std::vector<int>{TIOCM_DTR}));
  }
  std::puts("PASS production DTR/RTS boundary: order, EINVAL, ENOTTY, EIO");
}

#include "serial.h"
#include <cerrno>
#include <system_error>
#include <sys/ioctl.h>

namespace ublox_serial {
void raise_modem_lines(int fd) {
  for (int flag : {TIOCM_DTR, TIOCM_RTS}) {
    if (ioctl(fd, TIOCMBIS, &flag) < 0) {
      if (errno == EINVAL || errno == ENOTTY) return;
      throw std::system_error(errno, std::generic_category(), "serial modem lines");
    }
  }
}
}

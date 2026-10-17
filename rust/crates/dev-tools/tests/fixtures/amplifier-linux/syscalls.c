// Raw synchronous syscall boundary for the independent Rust source policy.
// No action ordering, retries, exception precedence, or captured answers live here.
#include <errno.h>
#include <fcntl.h>
#include <linux/i2c-dev.h>
#include <linux/i2c.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  int fd = -1;
  char operation;
  while (scanf(" %c", &operation) == 1) {
    int result = 0, value = 0;
    if (operation == 'O') {
      if (fd >= 0) return 3;
      fd = open(argv[1], O_RDWR);
      result = fd;
    } else if (operation == 'A') {
      unsigned address;
      if (fd < 0 || scanf(" %u", &address) != 1) return 3;
      result = ioctl(fd, I2C_SLAVE_FORCE, (unsigned long)address);
    } else if (operation == 'B') {
      unsigned direction, command, byte;
      if (fd < 0 || scanf(" %u %u %u", &direction, &command, &byte) != 3 ||
          direction > 1 || command > 255 || byte > 255) return 3;
      union i2c_smbus_data data;
      memset(&data, 0, sizeof(data));
      data.byte = (unsigned char)byte;
      struct i2c_smbus_ioctl_data request = {
          .read_write = (unsigned char)direction,
          .command = (unsigned char)command,
          .size = I2C_SMBUS_BYTE_DATA,
          .data = &data};
      result = ioctl(fd, I2C_SMBUS, &request);
      value = data.byte;
    } else if (operation == 'C') {
      if (fd < 0) return 3;
      result = close(fd);
      fd = -1;
    } else if (operation == 'Q') {
      return fd < 0 ? 0 : 3;
    } else {
      return 3;
    }
    const int error = result < 0 ? errno : 0;
    if (printf("%d %d\n", error, value) < 0 || fflush(stdout)) return 4;
  }
  return fd < 0 ? 0 : 3;
}

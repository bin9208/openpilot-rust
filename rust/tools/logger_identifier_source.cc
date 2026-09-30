#include <iostream>
#include "system/loggerd/logger.h"
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  std::cout << logger_get_identifier(argv[1]) << std::endl;
}

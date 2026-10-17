// Narrow unchanged-Params entrypoint. No candidate policy or fixture outcomes.
#include "common/params.h"
#include "common/swaglog.h"
#include <cstdarg>
#include <cstdio>
#include <iostream>
#include <string>
#include <sys/resource.h>
#include <sys/prctl.h>
void cloudlog_e(int, const char *, int, const char *, const char *format, ...) {
  va_list args; va_start(args, format); vfprintf(stderr, format, args); va_end(args);
}
int main(int argc, char **argv) {
  if (argc != 3 && argc != 4) return 2;
  rlimit core{0, 0}; if (setrlimit(RLIMIT_CORE, &core)) return 3;
  // A pipe core_pattern can invoke a system Python crash collector even with
  // RLIMIT_CORE=0. Keep the same SIGABRT and stderr without starting that
  // external collector. This affects only this QA entrypoint, never Params.
  if (prctl(PR_SET_DUMPABLE, 0)) return 3;
  setenv("OPENPILOT_PREFIX", "fixture", 1);
  Params params(argv[1]);
  if (argc == 4 && std::string(argv[2]) == "fatal") {
    // Preserve uncaught std::stoi SIGABRT, including native diagnostic bytes.
    std::cout << params.getInt(argv[3]) << std::endl;
    return 0;
  }
  if (std::string(argv[2]) != "serve") return 2;
  std::cout << "ready" << std::endl;
  for (std::string line; std::getline(std::cin, line);) {
    auto tab = line.find('\t'); if (tab == std::string::npos) return 4;
    auto operation = line.substr(0, tab), key = line.substr(tab + 1);
    if (!params.checkKey(key)) { std::cout << "unknown" << std::endl; continue; }
    if (operation == "boolean") std::cout << (params.getBool(key) ? "true" : "false") << std::endl;
    else if (operation == "integer") std::cout << params.getInt(key) << std::endl;
    else if (operation == "text") {
      const char digits[] = "0123456789abcdef";
      auto value = params.get(key); for (unsigned char byte : value) std::cout << digits[byte >> 4] << digits[byte & 15];
      std::cout << std::endl;
    } else return 4;
  }
}

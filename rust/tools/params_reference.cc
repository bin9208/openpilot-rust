#include "common/params.h"
#include "common/swaglog.h"
#include <algorithm>
#include <cstdarg>
#include <cstdio>
#include <iostream>
#include <iterator>

void cloudlog_e(int, const char *, int, const char *, const char *format, ...) {
  va_list args;
  va_start(args, format);
  vfprintf(stderr, format, args);
  va_end(args);
}

int main(int argc, char **argv) {
  if (argc != 5) return 2;
  setenv("OPENPILOT_PREFIX", argv[2], 1);
  Params params(argv[1]);
  const std::string operation(argv[3]), key(argv[4]);
  if (operation == "put") {
    std::string value((std::istreambuf_iterator<char>(std::cin)), std::istreambuf_iterator<char>());
    return params.put(key, value) == 0 ? 0 : 1;
  }
  if (operation == "get") { std::cout << params.get(key); return 0; }
  if (operation == "remove") return params.remove(key) == 0 ? 0 : 1;
  if (operation == "clear") { params.clearAll(static_cast<ParamKeyFlag>(std::stoul(key))); return 0; }
  if (operation == "catalog") {
    auto keys = params.allKeys();
    std::sort(keys.begin(), keys.end());
    for (const auto &name : keys) {
      auto value = params.getKeyDefaultValue(name);
      std::cout << name << '\t' << static_cast<uint32_t>(params.getKeyFlag(name)) << '\t'
                << params.getKeyType(name) << '\t' << value.has_value() << '\t' << value.value_or("") << '\n';
    }
    return 0;
  }
  return 2;
}

// Independent original Params::getInt boundary. No candidate parser or policy.
#include "common/params.h"
#include "common/swaglog.h"
#include <cstdarg>
#include <cstdio>
#include <iostream>
#include <sys/prctl.h>
#include <sys/resource.h>

// Match the existing Params source reference logging sink; the getInt path
// does not cast values or log. Do not conceal any unexpected diagnostic.
void cloudlog_e(int, const char *, int, const char *, const char *format, ...) {
  va_list args;
  va_start(args, format);
  vfprintf(stderr, format, args);
  va_end(args);
}

int main(int argc, char **argv) {
  if (argc != 2) return 2;
  const rlimit no_core{0, 0};
  if (setrlimit(RLIMIT_CORE, &no_core) != 0 || prctl(PR_SET_DUMPABLE, 0, 0, 0, 0) != 0) return 1;
  Params params(argv[1]);
  // Evaluate before printing, like original json.dumps(get_int(...)). Leave
  // std::stoi exceptions uncaught: SIGABRT and libstdc++ stderr are observed.
  const int value = params.getInt("SoundVolumeAdjust");
  std::cout << "{\"value\": " << value << "}\n";
  return 0;
}

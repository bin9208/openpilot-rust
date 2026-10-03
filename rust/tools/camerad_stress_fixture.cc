#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <fcntl.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

static void trace(const char* text, size_t length) {
  int saved = errno;
  const char* path = getenv("STRESS_TRACE");
  if (path) {
    int fd = syscall(SYS_openat, AT_FDCWD, path, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (fd < 0 || syscall(SYS_write, fd, text, length) != static_cast<long>(length)) _exit(90);
    syscall(SYS_close, fd);
  }
  errno = saved;
}

extern "C" int clock_gettime(clockid_t id, timespec* out) {
  if (id != CLOCK_BOOTTIME || !getenv("STRESS_CLOCK")) return syscall(SYS_clock_gettime, id, out);
  static const char* next = getenv("STRESS_CLOCK");
  if (!next || !*next) _exit(91);
  char* end = nullptr;
  out->tv_sec = strtoll(next, &end, 10);
  if (!end || *end != ':') _exit(92);
  out->tv_nsec = strtoll(end + 1, &end, 10);
  next = *end == ',' ? end + 1 : end;
  char line[160];
  int length = snprintf(line, sizeof(line), "{\"op\":\"clock\",\"id\":%d,\"sec\":%lld,\"nsec\":%lld}\n", id,
    static_cast<long long>(out->tv_sec), static_cast<long long>(out->tv_nsec));
  trace(line, length);
  return 0;
}

extern "C" int rand() noexcept {
  static auto real = reinterpret_cast<int (*)()>(dlsym(RTLD_NEXT, "rand"));
  static const char* next = getenv("STRESS_RAND");
  int value;
  if (next) {
    if (!*next) _exit(93);
    char* end = nullptr;
    long parsed = strtol(next, &end, 10);
    if (end == next || parsed < 0 || parsed > RAND_MAX) _exit(94);
    value = parsed;
    next = *end == ',' ? end + 1 : end;
  } else value = real();
  static bool mutated = false;
  if (!mutated) {
    if (const char* probability = getenv("STRESS_MUTATE_PROB")) setenv("SPECTRA_ERROR_PROB", probability, 1);
    if (const char* interval = getenv("STRESS_MUTATE_DT")) setenv("SPECTRA_ERROR_DT", interval, 1);
    mutated = true;
  }
  char line[96];
  int length = snprintf(line, sizeof(line), "{\"op\":\"rand\",\"value\":%d}\n", value);
  trace(line, length);
  return value;
}

extern "C" double strtod(const char* input, char** end) noexcept {
  static auto real = reinterpret_cast<double (*)(const char*, char**)>(dlsym(RTLD_NEXT, "strtod"));
  double result = real(input, end);
  int saved = errno;
  uint64_t bits;
  memcpy(&bits, &result, sizeof(bits));
  char hex[2048];
  size_t length = strlen(input);
  if (length * 2 >= sizeof(hex)) _exit(95);
  const char* digits = "0123456789abcdef";
  for (size_t i = 0; i < length; ++i) {
    unsigned char c = input[i];
    hex[2*i] = digits[c >> 4]; hex[2*i+1] = digits[c & 15];
  }
  hex[2*length] = 0;
  char line[2400];
  int count = snprintf(line, sizeof(line), "{\"op\":\"strtod\",\"input_hex\":\"%s\",\"bits\":%llu,\"end\":%lld,\"errno\":%d}\n",
    hex, static_cast<unsigned long long>(bits), end ? static_cast<long long>(*end - input) : -1LL, saved);
  trace(line, count);
  errno = saved;
  return result;
}

// Link-only QA shim: production source and inference code are unchanged.
#include <atomic>
#include <cerrno>
#include <cstdlib>
#include <cstdio>
#include <limits.h>
#include <string_view>
#include <unistd.h>
#include <zmq.h>

extern "C" int __real_zmq_msg_send(zmq_msg_t*, void*, int);
extern "C" int __wrap_zmq_msg_send(zmq_msg_t* message, void* socket, int flags) {
  const char* setting = std::getenv("MODEL_LOG_FAULT");
  if (!setting) return __real_zmq_msg_send(message, socket, flags);
  const void* data = zmq_msg_data(message);
  const size_t length = zmq_msg_size(message);
  const std::string_view mode(setting), packet(static_cast<const char*>(data), length);
  const bool timing = packet.find("\"event\": \"runtimeTiming\"") != std::string_view::npos;
  static std::atomic<unsigned> attempts{0};
  const bool eligible = (mode == "ordinary" && !timing) ||
                        ((mode == "timing-error" || mode == "timing-full") && timing);
  if (eligible && attempts.fetch_add(1) == 0) {
    const char* root = std::getenv("MODEL_LOG_FAULT_AUDIT");
    if (!root) std::abort();
    char path[PATH_MAX];
    const int count = std::snprintf(path, sizeof(path), "%s/%d.packet", root, getpid());
    if (count < 0 || static_cast<size_t>(count) >= sizeof(path)) std::abort();
    FILE* audit = std::fopen(path, "wb");
    if (!audit || std::fwrite(data, length, 1, audit) != 1 || std::fclose(audit) != 0) std::abort();
    errno = mode == "timing-full" ? EAGAIN : ENOTSOCK;
    return -1;
  }
  return __real_zmq_msg_send(message, socket, flags);
}

#include <atomic>
#include <cassert>
#include <cerrno>
#include <cinttypes>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
#include <fcntl.h>
#include <poll.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>
#include "third_party/linux/include/v4l2-controls.h"
#include <linux/videodev2.h>
#include <linux/ion.h>
#include <linux/msm_ion.h>
#include "common/queue.h"

#define LOGE(...) fprintf(stderr, __VA_ARGS__)
#define LOGW(...) fprintf(stderr, __VA_ARGS__)
#define LOGD(...) ((void)0)
#define HANDLE_EINTR(x) ({ decltype(x) ret; int count = 0; do { ret = (x); } while (ret == -1 && errno == EINTR && count++ < 100); ret; })
#define VISIONBUF_SYNC_FROM_DEVICE 0
#define VISIONBUF_SYNC_TO_DEVICE 1
#define BUF_IN_COUNT 7
#define BUF_OUT_COUNT 6
#define V4L2_QCOM_BUF_FLAG_CODECCONFIG 0x00020000
#define V4L2_QCOM_BUF_FLAG_EOS 0x02000000
const int env_debug_encoder = 0;
namespace capnp { using byte = uint8_t; }
namespace kj {
template<class T> using Array = std::vector<T>;
template<class T> struct ArrayPtr {
  T *pointer = nullptr; size_t length = 0;
  ArrayPtr(T *data, size_t size) : pointer(data), length(size) {}
  ArrayPtr(std::vector<T> &data) : pointer(data.data()), length(data.size()) {}
};
template<class T> ArrayPtr<T> arrayPtr(T *pointer, size_t length) { return {pointer, length}; }
template<class T> Array<T> heapArray(T *pointer, size_t length) { return {pointer, pointer + length}; }
}
namespace cereal { struct EncodeIndex { enum class Type { BIG_BOX_LOSSLESS = 0, FULL_H_E_V_C = 1, QCAMERA_H264 = 6 }; }; }
const auto MAIN_ENCODE_TYPE = cereal::EncodeIndex::Type::FULL_H_E_V_C;
struct VisionIpcBufExtra { uint32_t frame_id; uint64_t timestamp_sof, timestamp_eof; };
namespace util {
void set_thread_name(const char *) {}
void set_thread_name(const std::string &) {}
int safe_ioctl(int fd, unsigned long request, void *argument, const char *message = nullptr) {
  int result;
  do { result = ioctl(fd, request, argument); } while (result == -1 && errno == EINTR);
  if (result == -1 && message) throw std::runtime_error(message);
  return result;
}
std::string string_format(const char *format, ...) {
  char buffer[1024]; va_list args; va_start(args, format); vsnprintf(buffer, sizeof(buffer), format, args); va_end(args); return buffer;
}
}
static double millis_since_boot() { return 0; }
static std::filesystem::path directory;
static std::ofstream trace;
static std::string hex(kj::ArrayPtr<capnp::byte> bytes) {
  static const char digits[] = "0123456789abcdef";
  std::string result;
  for (size_t index = 0; index < bytes.length; ++index) {
    result += digits[bytes.pointer[index] >> 4]; result += digits[bytes.pointer[index] & 15];
  }
  return result;
}

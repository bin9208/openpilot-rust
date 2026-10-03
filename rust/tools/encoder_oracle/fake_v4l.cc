#include <algorithm>
#include <cerrno>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <dlfcn.h>
#include <fcntl.h>
#include <map>
#include <mutex>
#include <poll.h>
#include <string>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <thread>
#include <unistd.h>
#include "third_party/linux/include/v4l2-controls.h"
#include <linux/videodev2.h>
#include <linux/ion.h>
#include <linux/msm_ion.h>

struct Slot { unsigned index; unsigned long address; unsigned length; int fd; timeval time; };
struct Device {
  std::deque<Slot> capture, inputs, returned;
  std::map<unsigned, int> controls;
  bool header = true, stopped = false, eos = false, poll_fault = false, burst_released = false;
  unsigned frames = 0;
};
struct State {
  std::mutex lock;
  std::map<int, Device> devices;
  std::map<int, bool> ions;
  std::map<int, int> allocations;
  int next_handle = 1;
};
static State &state() { static auto *value = new State; return *value; }
static std::string scenario() { const char *value = getenv("ENCODER_FAKE_CASE"); return value ? value : "normal"; }
static void record(const char *format, ...) {
  const char *path = getenv("ENCODER_FAKE_TRACE");
  if (!path) return;
  FILE *file = fopen(path, "a");
  if (!file) abort();
  va_list arguments; va_start(arguments, format); vfprintf(file, format, arguments); va_end(arguments);
  fputc('\n', file); fclose(file);
}
static int fake_open(const char *path) {
  if (strcmp(path, "/dev/ion") && strcmp(path, "/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index1")) return -2;
  std::lock_guard guard(state().lock);
  int fd = syscall(SYS_memfd_create, "encoder-driver-fixture", 0);
  if (fd < 0) abort();
  if (!strcmp(path, "/dev/ion")) { state().ions[fd] = true; record("OPEN ion"); }
  else { state().devices[fd] = Device{}; record("OPEN v4l"); }
  return fd;
}
extern "C" int open(const char *path, int flags, ...) {
  const int fd = fake_open(path); if (fd != -2) return fd;
  mode_t mode = 0; if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); }
  static auto next = reinterpret_cast<int (*)(const char *, int, ...)>(dlsym(RTLD_NEXT, "open"));
  return next(path, flags, mode);
}
extern "C" int open64(const char *path, int flags, ...) {
  mode_t mode = 0; if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); }
  return open(path, flags, mode);
}
extern "C" int openat(int directory, const char *path, int flags, ...) {
  const int fd = fake_open(path); if (fd != -2) return fd;
  mode_t mode = 0; if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); }
  return syscall(SYS_openat, directory, path, flags, mode);
}
extern "C" int openat64(int directory, const char *path, int flags, ...) {
  mode_t mode = 0; if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); }
  return openat(directory, path, flags, mode);
}
extern "C" int close(int fd) {
  {
    std::lock_guard guard(state().lock);
    if (state().devices.erase(fd)) record("CLOSE v4l");
    if (state().ions.erase(fd)) record("CLOSE ion");
  }
  return syscall(SYS_close, fd);
}
static int reject() { errno = EINVAL; return -1; }
extern "C" int ioctl(int fd, unsigned long request, ...) {
  va_list args; va_start(args, request); void *argument = va_arg(args, void *); va_end(args);
  std::unique_lock guard(state().lock);
  if (state().ions.count(fd)) {
    if (request == ION_IOC_ALLOC) {
      auto *value = static_cast<ion_allocation_data *>(argument);
      if (value->align != 4096 || value->heap_id_mask != (1U << ION_IOMMU_HEAP_ID) || value->flags != ION_FLAG_CACHED) abort();
      const int allocation = syscall(SYS_memfd_create, "encoder-capture-fixture", 0);
      if (allocation < 0 || ftruncate(allocation, value->len)) abort();
      value->handle = state().next_handle++;
      state().allocations[value->handle] = allocation;
      record("ION_ALLOC %zu", value->len); return 0;
    }
    if (request == ION_IOC_SHARE) {
      auto *value = static_cast<ion_fd_data *>(argument);
      value->fd = dup(state().allocations.at(value->handle)); record("ION_SHARE"); return 0;
    }
    if (request == ION_IOC_FREE) {
      const int handle = static_cast<ion_handle_data *>(argument)->handle;
      syscall(SYS_close, state().allocations.at(handle)); state().allocations.erase(handle);
      record("ION_FREE"); return 0;
    }
    if (request == ION_IOC_CUSTOM) {
      auto *custom = static_cast<ion_custom_data *>(argument);
      auto *flush = reinterpret_cast<ion_flush_data *>(custom->arg);
      if (custom->cmd != ION_IOC_INV_CACHES || !state().allocations.count(flush->handle) || !flush->vaddr || flush->length != 4096 || flush->offset) abort();
      record("ION_SYNC"); return 0;
    }
    abort();
  }
  auto found = state().devices.find(fd);
  if (found == state().devices.end()) {
    guard.unlock();
    static auto next = reinterpret_cast<int (*)(int, unsigned long, ...)>(dlsym(RTLD_NEXT, "ioctl"));
    return next(fd, request, argument);
  }
  Device &device = found->second;
  const auto test = scenario();
  if (request == VIDIOC_QUERYCAP) {
    auto *value = static_cast<v4l2_capability *>(argument); memset(value, 0, sizeof(*value));
    strcpy(reinterpret_cast<char *>(value->driver), test == "wrong-capability" ? "other" : "msm_vidc_driver");
    strcpy(reinterpret_cast<char *>(value->card), "msm_vidc_venc"); record("QUERYCAP"); return 0;
  }
  if (request == VIDIOC_S_FMT) {
    auto *value = static_cast<v4l2_format *>(argument);
    record("FORMAT %u %u %u %u %u", value->type, value->fmt.pix_mp.width, value->fmt.pix_mp.height, value->fmt.pix_mp.pixelformat, value->fmt.pix_mp.colorspace);
    value->fmt.pix_mp.plane_fmt[0].sizeimage = 4096;
    if (test == "wrong-output" && value->type == V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE) value->fmt.pix_mp.height += 2;
    return 0;
  }
  if (request == VIDIOC_S_PARM) {
    auto *value = static_cast<v4l2_streamparm *>(argument);
    record("FPS %u %u %u", value->type, value->parm.output.timeperframe.numerator, value->parm.output.timeperframe.denominator); return 0;
  }
  if (request == VIDIOC_S_SELECTION) {
    auto *value = static_cast<v4l2_selection *>(argument);
    record("CROP %u %u %d %d %u %u", value->type, value->target, value->r.left, value->r.top, value->r.width, value->r.height);
    if (test == "crop-unavailable") return reject();
    if (test == "crop-adjusted") value->r.top += 2;
    return 0;
  }
  if (request == VIDIOC_S_CTRL) {
    auto *value = static_cast<v4l2_control *>(argument); record("CONTROL %u %d", value->id, value->value);
    if ((test == "slice-size-failure" && value->id == V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MAX_BYTES)
        || (test == "slice-mode-failure" && value->id == V4L2_CID_MPEG_VIDEO_MULTI_SLICE_MODE && value->value == V4L2_MPEG_VIDEO_MULTI_SICE_MODE_MAX_BYTES)
        || (test == "compat-failure" && ((value->id == V4L2_CID_MPEG_VIDEO_H264_PROFILE && value->value == V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE)
            || (value->id == V4L2_CID_MPEG_VIDEO_H264_ENTROPY_MODE && value->value == V4L2_MPEG_VIDEO_H264_ENTROPY_MODE_CAVLC)))) return reject();
    device.controls[value->id] = value->value; return 0;
  }
  if (request == VIDIOC_G_CTRL) {
    auto *value = static_cast<v4l2_control *>(argument); record("READ_CONTROL %u", value->id);
    if (test == "cbr-unavailable") return reject();
    value->value = device.controls.at(value->id);
    if (test == "cbr-wrong" && value->id == V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL) value->value = V4L2_CID_MPEG_VIDC_VIDEO_RATE_CONTROL_VBR_CFR;
    if (test == "bitrate-adjusted" && value->id == V4L2_CID_MPEG_VIDEO_BITRATE) value->value /= 2;
    return 0;
  }
  if (request == VIDIOC_REQBUFS) {
    auto *value = static_cast<v4l2_requestbuffers *>(argument);
    if (value->memory != V4L2_MEMORY_USERPTR) abort();
    record("BUFFERS %u %u", value->type, value->count); return 0;
  }
  if (request == VIDIOC_STREAMON || request == VIDIOC_STREAMOFF) {
    record("STREAM %d %u", request == VIDIOC_STREAMON, *static_cast<unsigned *>(argument)); return 0;
  }
  if (request == VIDIOC_QBUF) {
    auto *value = static_cast<v4l2_buffer *>(argument); auto *plane = value->m.planes;
    if (test == "input-queue-failure" && value->type == V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE) return reject();
    if (value->memory != V4L2_MEMORY_USERPTR || value->length != 1 || value->flags != V4L2_BUF_FLAG_TIMESTAMP_COPY || plane->bytesused != plane->length || !plane->m.userptr) abort();
    Slot slot{value->index, plane->m.userptr, plane->length, int(plane->reserved[0]), value->timestamp};
    if (fcntl(slot.fd, F_GETFD) < 0) abort();
    if (value->type == V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE) {
      device.capture.push_back(slot); record("QUEUE_CAPTURE %u %u", slot.index, slot.length);
    } else {
      if (device.stopped) { device.stopped = false; device.eos = false; device.header = true; device.frames = 0; }
      device.inputs.push_back(slot);
      record("QUEUE_INPUT %u %u %ld %ld", slot.index, slot.length, slot.time.tv_sec, slot.time.tv_usec);
    }
    return 0;
  }
  if (request == VIDIOC_DQBUF) {
    auto *value = static_cast<v4l2_buffer *>(argument); auto *plane = value->m.planes;
    if (value->type == V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE) {
      if (device.returned.empty()) abort();
      const Slot input = device.returned.front(); device.returned.pop_front();
      if (fcntl(input.fd, F_GETFD) < 0) abort();
      value->index = input.index; plane->data_offset = 0;
      record("RETURN_INPUT %u", input.index); return 0;
    }
    if (device.capture.empty()) abort();
    const Slot capture = device.capture.front(); device.capture.pop_front();
    value->index = capture.index; plane->data_offset = test == "wrong-offset" ? 1 : 0;
    if (device.header) {
      device.header = false; value->flags = 0x00020000; plane->bytesused = 8;
      memcpy(reinterpret_cast<void *>(capture.address), "HEADER12", 8); record("CAPTURE_HEADER");
    } else if (!device.inputs.empty()) {
      const Slot input = device.inputs.front(); device.inputs.pop_front();
      if (fcntl(input.fd, F_GETFD) < 0 || input.length < 64) abort();
      value->timestamp = input.time;
      if (test == "wrong-timestamp") value->timestamp.tv_usec += 1;
      value->flags = device.frames++ % 3 == 0 ? V4L2_BUF_FLAG_KEYFRAME : 0;
      plane->bytesused = 64; memcpy(reinterpret_cast<void *>(capture.address), reinterpret_cast<const void *>(input.address), 64);
      device.returned.push_back(input); record("CAPTURE_FRAME %ld %ld %u", input.time.tv_sec, input.time.tv_usec, value->flags);
    } else if (device.stopped && !device.eos) {
      device.eos = true; value->flags = 0x02000000; plane->bytesused = 0; record("CAPTURE_EOS");
    } else abort();
    return 0;
  }
  if (request == VIDIOC_ENCODER_CMD) {
    if (static_cast<v4l2_encoder_cmd *>(argument)->cmd != V4L2_ENC_CMD_STOP || !device.inputs.empty() || !device.returned.empty()) abort();
    device.stopped = true; record("STOP"); return 0;
  }
  record("UNKNOWN %lu", request); abort();
}
extern "C" int poll(pollfd *fds, nfds_t count, int timeout) {
  {
    std::unique_lock guard(state().lock);
    if (count == 1 && state().devices.count(fds[0].fd)) {
      auto &device = state().devices.at(fds[0].fd);
      if (scenario() == "poll-eintr" && !device.poll_fault) { device.poll_fault = true; errno = EINTR; return -1; }
      fds[0].revents = 0;
      if (!device.capture.empty() && (device.header || !device.inputs.empty() || (device.stopped && !device.eos))) fds[0].revents |= POLLIN;
      if (device.returned.size() >= 7) device.burst_released = true;
      if (!device.returned.empty() && (scenario() != "hold-seven" || device.burst_released)) fds[0].revents |= POLLOUT;
      if (fds[0].revents) return 1;
    } else {
      guard.unlock();
      static auto next = reinterpret_cast<int (*)(pollfd *, nfds_t, int)>(dlsym(RTLD_NEXT, "poll"));
      return next(fds, count, timeout);
    }
  }
  std::this_thread::sleep_for(std::chrono::milliseconds(1));
  return 0;
}

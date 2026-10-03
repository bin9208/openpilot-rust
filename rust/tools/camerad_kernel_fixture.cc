#include <cerrno>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <poll.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <media/cam_defs.h>
#include <media/cam_req_mgr.h>
#include <media/cam_sync.h>

static int tracked[2048], log_fd = -1, allocated = 0, imports = 0, syncs = 0, failures = 0;
static void* mappings[2048];
static size_t lengths[2048];
static void trace(const char* format, ...) {
  int saved = errno;
  if (log_fd < 0) {
    const char* path = getenv("CK_TRACE");
    if (path) log_fd = syscall(SYS_openat, AT_FDCWD, path, O_WRONLY | O_CREAT | O_APPEND, 0600);
  }
  if (log_fd >= 0) {
    char bytes[16384]; va_list args; va_start(args, format);
    int n = vsnprintf(bytes, sizeof(bytes), format, args); va_end(args);
    if (n <= 0 || n >= static_cast<int>(sizeof(bytes))) abort();
    syscall(SYS_write, log_fd, bytes, n); syscall(SYS_write, log_fd, "\n", 1);
  }
  errno = saved;
}
static int setting(const char* key, int value) { const char* text = getenv(key); return text ? atoi(text) : value; }
static bool failure(const char* name, int& ret, int& error) {
  const char* wanted = getenv("CK_FAIL_OP");
  if (!wanted || strcmp(wanted, name)) return false;
  int skip = setting("CK_FAIL_SKIP", 0), count = setting("CK_FAIL_COUNT", 1), attempt = failures++;
  if (attempt < skip || (count >= 0 && attempt >= skip + count)) return false;
  ret = setting("CK_FAIL_CODE", -1); error = setting("CK_FAIL_ERRNO", EIO); return true;
}
static void hex(const void* data, size_t size, char* output) {
  const auto* bytes = static_cast<const uint8_t*>(data);
  for (size_t i = 0; i < size; ++i) snprintf(output + 2 * i, 3, "%02x", bytes[i]);
  output[2 * size] = 0;
}
static int memfile(int fd, size_t length) {
  int temporary = syscall(SYS_memfd_create, "camera-kernel-fixture", 0);
  if (temporary < 0 || syscall(SYS_ftruncate, temporary, length)) abort();
  if (temporary != fd) { if (syscall(SYS_dup3, temporary, fd, 0) < 0) abort(); syscall(SYS_close, temporary); }
  return fd;
}
static int opened(const char* path, int flags, mode_t mode) {
  int fd = !strcmp(path, "/dev/camera-fixture-request") ? 501 : !strcmp(path, "/dev/camera-fixture-raw") ? 601 : !strcmp(path, "/dev/camera-fixture-yuv") ? 602 : -1;
  if (fd < 0) {
    if (!strncmp(path, "/dev/v4l", 8) || !strncmp(path, "/sys/class/video4linux", 21)) { errno = EACCES; return -1; }
    return syscall(SYS_openat, AT_FDCWD, path, flags, mode);
  }
  memfile(fd, 4096); tracked[fd] = 1;
  trace("{\"op\":\"open\",\"fd\":%d,\"flags\":%d}", fd, flags); errno = 0; return fd;
}
extern "C" int open(const char* path, int flags, ...) {
  mode_t mode = 0; if (flags & O_CREAT) { va_list a; va_start(a, flags); mode = va_arg(a, int); va_end(a); }
  return opened(path, flags, mode);
}
extern "C" int open64(const char* path, int flags, ...) {
  mode_t mode = 0; if (flags & O_CREAT) { va_list a; va_start(a, flags); mode = va_arg(a, int); va_end(a); }
  return opened(path, flags, mode);
}
extern "C" int close(int fd) {
  if (fd >= 0 && fd < 2048 && tracked[fd]) { trace("{\"op\":\"close\",\"fd\":%d}", fd); tracked[fd] = 0; }
  return syscall(SYS_close, fd);
}
extern "C" void* mmap(void* address, size_t length, int prot, int flags, int fd, off_t offset) noexcept {
  bool owned = fd >= 0 && fd < 2048 && tracked[fd] == 2;
  int ret = 0, error = 0;
  if (owned && failure("mmap", ret, error)) {
    trace("{\"op\":\"mmap\",\"fd\":%d,\"length\":%zu,\"prot\":%d,\"flags\":%d,\"offset\":%ld,\"ok\":false,\"errno\":%d}", fd, length, prot, flags, offset, error);
    errno = error; return MAP_FAILED;
  }
  void* p = reinterpret_cast<void*>(syscall(SYS_mmap, address, length, prot, flags, fd, offset));
  if (owned) {
    trace("{\"op\":\"mmap\",\"fd\":%d,\"length\":%zu,\"prot\":%d,\"flags\":%d,\"offset\":%ld,\"ok\":%s,\"errno\":%d}", fd, length, prot, flags, offset, p == MAP_FAILED ? "false" : "true", p == MAP_FAILED ? errno : 0);
    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; }
  }
  return p;
}
extern "C" void* mmap64(void* a, size_t n, int p, int f, int fd, off64_t o) noexcept { return mmap(a, n, p, f, fd, o); }
extern "C" int munmap(void* address, size_t length) noexcept {
  for (int fd = 0; fd < 2048; ++fd) if (mappings[fd] == address && lengths[fd]) {
    if (length > 4096) abort();
    char data[8193]; hex(address, length, data);
    trace("{\"op\":\"munmap\",\"fd\":%d,\"length\":%zu,\"data\":\"%s\"}", fd, length, data);
    mappings[fd] = nullptr; lengths[fd] = 0; break;
  }
  return syscall(SYS_munmap, address, length);
}
extern "C" int poll(struct pollfd* fds, nfds_t count, int timeout) {
  if (count != 1 || fds[0].fd != 501) {
    struct timespec duration{timeout / 1000, (timeout % 1000) * 1000000L};
    return syscall(SYS_ppoll, fds, count, timeout < 0 ? nullptr : &duration, nullptr, 0);
  }
  int ret = setting("CK_POLL_RETURN", 1), error = 0;
  failure("poll", ret, error);
  short before = fds[0].revents;
  fds[0].revents = setting("CK_POLL_EVENTS", POLLPRI);
  trace("{\"op\":\"poll\",\"fd\":501,\"count\":1,\"timeout\":%d,\"events\":%d,\"before\":%d,\"after\":%d,\"ret\":%d,\"errno\":%d}", timeout, fds[0].events, before, fds[0].revents, ret, error);
  errno = error; return ret;
}
extern "C" int ioctl(int fd, unsigned long request, ...) noexcept {
  va_list args; va_start(args, request); void* payload = va_arg(args, void*); va_end(args);
  if (fd != 501) { errno = ENOTTY; return -1; }
  if (request == VIDIOC_DQEVENT) {
    char before[273], after[273]; hex(payload, 136, before);
    int ret = 0, error = 0; bool failed = failure("event", ret, error);
    if (!failed || setting("CK_MUTATE_FAILURE", 0)) {
      auto* event = static_cast<v4l2_event*>(payload);
      event->type = 0x08000000; event->id = 2;
      auto* data = reinterpret_cast<cam_req_mgr_message*>(event->u.data);
      data->session_hdl = -17; data->u.frame_msg.link_hdl = -31;
      data->u.frame_msg.request_id = UINT64_C(0xdeadbeef01234567);
      data->u.frame_msg.frame_id = UINT64_C(0xfedcba9876543210);
      data->u.frame_msg.timestamp = UINT64_C(0x1122334455667788);
      data->u.frame_msg.sof_status = 7;
    }
    hex(payload, 136, after);
    trace("{\"op\":\"event\",\"request\":%lu,\"before\":\"%s\",\"after\":\"%s\",\"ret\":%d,\"errno\":%d}", request, before, after, ret, error);
    errno = error; return ret;
  }
  if (request != VIDIOC_CAM_CONTROL) abort();
  auto* outer = static_cast<cam_control*>(payload);
  bool handle_only = outer->op_code == 0x10a;
  bool sync = outer->op_code <= 6 && !(outer->op_code == 0 && outer->handle_type == 1);
  uint8_t normalized[24]; memcpy(normalized, payload, 24);
  if (!handle_only) { memset(normalized + 16, 0, 8); normalized[16] = 1; }
  char outer_before[49], outer_after[49], before[1025], after[1025], nested[265] = {};
  hex(normalized, 24, outer_before);
  size_t size = handle_only ? 0 : outer->size;
  if (size > 512) abort();
  auto* data = reinterpret_cast<uint8_t*>(outer->handle);
  uint8_t copy[512];
  if (size) memcpy(copy, data, size);
  bool acquire = outer->op_code == CAM_ACQUIRE_DEV;
  if (acquire) {
    auto* command = reinterpret_cast<cam_acquire_dev_cmd*>(data);
    if (command->resource_hdl) {
      hex(reinterpret_cast<void*>(command->resource_hdl), 8, nested);
      memset(copy + 16, 0, 8); copy[16] = 2;
    }
  }
  hex(copy, size, before);
  char name[64]; snprintf(name, sizeof(name), "%s:%u", sync ? "sync" : "camera", outer->op_code);
  int ret = 0, error = 0; bool failed = failure(name, ret, error);
  if (!failed || setting("CK_MUTATE_FAILURE", 0)) {
    if (outer->op_code == CAM_REQ_MGR_CREATE_SESSION) reinterpret_cast<cam_req_mgr_session_info*>(data)->session_hdl = -1001;
    if (acquire) reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = -1003;
    if (outer->op_code == CAM_REQ_MGR_LINK) reinterpret_cast<cam_req_mgr_link_info*>(data)->link_hdl = -1005;
    if (outer->op_code == CAM_REQ_MGR_MAP_BUF) {
      auto* command = reinterpret_cast<cam_mem_mgr_map_cmd*>(data);
      command->out.buf_handle = 0x98760000 + ++imports; command->out.vaddr = UINT64_C(0x0102030405060708);
    }
    if (outer->op_code == CAM_REQ_MGR_ALLOC_BUF) {
      auto* command = reinterpret_cast<cam_mem_mgr_alloc_cmd*>(data);
      int export_fd = 700 + ++allocated;
      memfile(export_fd, command->len); tracked[export_fd] = 2;
      uint8_t bytes[4096]; memset(bytes, 0xa5, sizeof(bytes));
      if (command->len > sizeof(bytes) || syscall(SYS_pwrite64, export_fd, bytes, command->len, 0) != static_cast<ssize_t>(command->len)) abort();
      command->out.fd = export_fd;
      command->out.buf_handle = (export_fd << 16) | allocated;
      command->out.vaddr = UINT64_C(0xa1b2c3d4e5f60718);
    }
    if (outer->op_code == CAM_SYNC_CREATE && size == sizeof(cam_sync_info)) reinterpret_cast<cam_sync_info*>(data)->sync_obj = 4000 + ++syncs;
    if (sync) reinterpret_cast<cam_private_ioctl_arg*>(payload)->result = setting("CK_SYNC_RESULT", 0);
  }
  if (size) memcpy(copy, data, size);
  if (acquire && reinterpret_cast<cam_acquire_dev_cmd*>(data)->resource_hdl) { memset(copy + 16, 0, 8); copy[16] = 2; }
  hex(copy, size, after);
  memcpy(normalized, payload, 24); if (!handle_only) { memset(normalized + 16, 0, 8); normalized[16] = 1; }
  hex(normalized, 24, outer_after);
  trace("{\"op\":\"ioctl\",\"name\":\"%s\",\"request\":%lu,\"outer_before\":\"%s\",\"before\":\"%s\",\"nested\":\"%s\",\"outer_after\":\"%s\",\"after\":\"%s\",\"ret\":%d,\"errno\":%d}", name, request, outer_before, before, nested, outer_after, after, ret, error);
  errno = error; return ret;
}

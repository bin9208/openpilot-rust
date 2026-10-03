#include <cerrno>
#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <fcntl.h>
#include <linux/ion.h>
#include <linux/msm_ion.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

struct Object { dev_t device; ino_t inode; size_t length; };
struct Handle { int value; int fd; int object; bool live; };
struct Mapping { void *address; size_t length; int object; bool live; };
static Object objects[128];
static Handle handles[128];
static Mapping mappings[128];
static int object_count;
static int handle_count;
static int mapping_count;
static int device_fd = -1;
static int trace_fd = -1;
static int matching_calls;
static int failures;

template <class T> T symbol(const char *name) { return reinterpret_cast<T>(dlsym(RTLD_NEXT, name)); }

static int real_close(int fd) { return symbol<int (*)(int)>("close")(fd); }

static void record(const char *format, ...) {
  const int saved_errno = errno;
  if (trace_fd < 0) {
    const char *path = std::getenv("IPC_ION_TRACE");
    if (!path) return;
    trace_fd = symbol<int (*)(const char *, int, ...)>("open")(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
    if (trace_fd < 0) _exit(91);
  }
  char line[1024];
  va_list arguments;
  va_start(arguments, format);
  const int length = std::vsnprintf(line, sizeof(line), format, arguments);
  va_end(arguments);
  if (length < 0 || static_cast<size_t>(length) >= sizeof(line)) _exit(92);
  if (syscall(SYS_write, trace_fd, line, static_cast<size_t>(length)) != length) _exit(93);
  errno = saved_errno;
}

static int number(const char *name, int fallback) {
  const char *value = std::getenv(name);
  return value ? std::atoi(value) : fallback;
}

static bool fail(const char *operation) {
  const char *target = std::getenv("IPC_ION_FAIL");
  if (!target || std::strcmp(target, operation)) return false;
  if (matching_calls++ < number("IPC_ION_SKIP", 0)) return false;
  if (failures >= number("IPC_ION_COUNT", 1)) return false;
  ++failures;
  errno = number("IPC_ION_ERRNO", EIO);
  record("{\"op\":\"%s\",\"error\":%d,\"result\":%d}\n", operation, errno, number("IPC_ION_RETURN", -1));
  return true;
}

static int object_for_fd(int fd) {
  if (fd < 0) return -1;
  struct stat metadata{};
  if (fstat(fd, &metadata)) return -1;
  for (int i = 0; i < object_count; ++i) {
    if (objects[i].device == metadata.st_dev && objects[i].inode == metadata.st_ino) return i;
  }
  return -1;
}

static Handle *handle_for(int value) {
  for (int i = 0; i < handle_count; ++i) if (handles[i].live && handles[i].value == value) return &handles[i];
  errno = EINVAL;
  return nullptr;
}

static int open_path(const char *path, int flags, mode_t mode, const char *function) {
  auto next = symbol<int (*)(const char *, int, ...)>(function);
  if (std::strcmp(path, "/dev/ion")) return next(path, flags, mode);
  if (fail("open")) return -1;
  device_fd = next("/dev/null", flags, mode);
  record("{\"op\":\"open\",\"flags\":%d}\n", flags & ~O_CLOEXEC);
  return device_fd;
}

extern "C" int open(const char *path, int flags, ...) {
  va_list arguments;
  va_start(arguments, flags);
  const mode_t mode = flags & O_CREAT ? va_arg(arguments, mode_t) : 0;
  va_end(arguments);
  return open_path(path, flags, mode, "open");
}

extern "C" int open64(const char *path, int flags, ...) {
  va_list arguments;
  va_start(arguments, flags);
  const mode_t mode = flags & O_CREAT ? va_arg(arguments, mode_t) : 0;
  va_end(arguments);
  return open_path(path, flags, mode, "open64");
}

extern "C" int close(int fd) {
  if (fd == device_fd) { record("{\"op\":\"device_close\"}\n"); device_fd = -1; }
  return real_close(fd);
}

extern "C" int ioctl(int fd, unsigned long request, ...) {
  va_list arguments;
  va_start(arguments, request);
  void *argument = va_arg(arguments, void *);
  va_end(arguments);
  if (fd != device_fd) return symbol<int (*)(int, unsigned long, ...)>("ioctl")(fd, request, argument);
  if (request == ION_IOC_ALLOC) {
    if (fail("allocate")) return number("IPC_ION_RETURN", -1);
    auto &allocation = *static_cast<ion_allocation_data *>(argument);
    if (allocation.align != 4096 || allocation.heap_id_mask != (1U << ION_IOMMU_HEAP_ID) || allocation.flags != ION_FLAG_CACHED || allocation.len < 8 || object_count >= 128 || handle_count >= 128) { errno = EINVAL; return -1; }
    const int backing = static_cast<int>(syscall(SYS_memfd_create, "ipc-ion-fixture", 0));
    if (backing < 0 || ftruncate(backing, static_cast<off_t>(allocation.len))) return -1;
    uint8_t poisoned[4096];
    std::memset(poisoned, 0xa5, sizeof(poisoned));
    for (size_t offset = 0; offset < allocation.len; offset += sizeof(poisoned)) {
      const size_t count = allocation.len - offset < sizeof(poisoned) ? allocation.len - offset : sizeof(poisoned);
      if (pwrite(backing, poisoned, count, static_cast<off_t>(offset)) != static_cast<ssize_t>(count)) return -1;
    }
    struct stat metadata{};
    if (fstat(backing, &metadata)) return -1;
    const int object = object_count++;
    objects[object] = {metadata.st_dev, metadata.st_ino, allocation.len};
    const int index = handle_count++;
    handles[index] = {100 + index, backing, object, true};
    allocation.handle = handles[index].value;
    record("{\"op\":\"allocate\",\"handle\":%d,\"object\":%d,\"length\":%zu,\"align\":%zu,\"heap\":%u,\"flags\":%u}\n", allocation.handle, object, allocation.len, allocation.align, allocation.heap_id_mask, allocation.flags);
    return 0;
  }
  if (request == ION_IOC_SHARE) {
    if (fail("share")) return number("IPC_ION_RETURN", -1);
    auto &descriptor = *static_cast<ion_fd_data *>(argument);
    Handle *handle = handle_for(descriptor.handle);
    if (!handle) return -1;
    descriptor.fd = dup(handle->fd);
    if (descriptor.fd < 0) return -1;
    record("{\"op\":\"share\",\"handle\":%d,\"object\":%d}\n", handle->value, handle->object);
    return 0;
  }
  if (request == ION_IOC_IMPORT) {
    if (fail("import")) return number("IPC_ION_RETURN", -1);
    auto &descriptor = *static_cast<ion_fd_data *>(argument);
    const int object = object_for_fd(descriptor.fd);
    if (object < 0 || handle_count >= 128) { errno = EINVAL; return -1; }
    const int copied = dup(descriptor.fd);
    if (copied < 0) return -1;
    const int index = handle_count++;
    handles[index] = {100 + index, copied, object, true};
    descriptor.handle = handles[index].value;
    record("{\"op\":\"import\",\"handle\":%d,\"object\":%d}\n", descriptor.handle, object);
    return 0;
  }
  if (request == ION_IOC_CUSTOM) {
    const auto &custom = *static_cast<ion_custom_data *>(argument);
    const char *operation = custom.cmd == ION_IOC_INV_CACHES ? "invalidate" : custom.cmd == ION_IOC_CLEAN_CACHES ? "clean" : "invalid-cache-command";
    if (fail(operation)) return number("IPC_ION_RETURN", -1);
    const auto &flush = *reinterpret_cast<const ion_flush_data *>(custom.arg);
    Handle *handle = handle_for(flush.handle);
    if (!handle) return -1;
    bool mapped = false;
    for (int i = 0; i < mapping_count; ++i) mapped |= mappings[i].live && mappings[i].address == flush.vaddr && mappings[i].object == handle->object;
    if (!mapped || flush.fd || flush.offset || flush.length != objects[handle->object].length - 8 || (custom.cmd != ION_IOC_INV_CACHES && custom.cmd != ION_IOC_CLEAN_CACHES)) { errno = EINVAL; return -1; }
    record("{\"op\":\"%s\",\"handle\":%d,\"object\":%d,\"length\":%u,\"offset\":%u,\"command\":%u,\"first_byte\":%u}\n", operation, handle->value, handle->object, flush.length, flush.offset, custom.cmd, *static_cast<uint8_t *>(flush.vaddr));
    return 0;
  }
  if (request == ION_IOC_FREE) {
    if (fail("free")) return number("IPC_ION_RETURN", -1);
    const auto &value = *static_cast<ion_handle_data *>(argument);
    Handle *handle = handle_for(value.handle);
    if (!handle) return -1;
    real_close(handle->fd);
    handle->live = false;
    record("{\"op\":\"free\",\"handle\":%d,\"object\":%d}\n", handle->value, handle->object);
    return 0;
  }
  record("{\"op\":\"unknown_ioctl\",\"request\":%lu}\n", request);
  errno = ENOTTY;
  return -1;
}

static void *map_region(void *address, size_t length, int protection, int flags, int fd, off_t offset, const char *function) {
  const int object = object_for_fd(fd);
  if (object >= 0 && fail("mmap")) return MAP_FAILED;
  void *result = symbol<void *(*)(void *, size_t, int, int, int, off_t)>(function)(address, length, protection, flags, fd, offset);
  if (object >= 0 && result != MAP_FAILED) {
    if (mapping_count >= 128) _exit(94);
    mappings[mapping_count++] = {result, length, object, true};
    record("{\"op\":\"mmap\",\"object\":%d,\"length\":%zu,\"protection\":%d,\"flags\":%d,\"offset\":%lld}\n", object, length, protection, flags, static_cast<long long>(offset));
  }
  return result;
}

extern "C" void *mmap(void *address, size_t length, int protection, int flags, int fd, off_t offset) {
  return map_region(address, length, protection, flags, fd, offset, "mmap");
}

extern "C" void *mmap64(void *address, size_t length, int protection, int flags, int fd, off64_t offset) {
  return map_region(address, length, protection, flags, fd, offset, "mmap64");
}

extern "C" int munmap(void *address, size_t length) {
  const int result = symbol<int (*)(void *, size_t)>("munmap")(address, length);
  for (int i = 0; i < mapping_count; ++i) {
    if (mappings[i].live && mappings[i].address == address && mappings[i].length == length && !result) {
      mappings[i].live = false;
      record("{\"op\":\"munmap\",\"object\":%d,\"length\":%zu}\n", mappings[i].object, length);
      break;
    }
  }
  return result;
}

__attribute__((destructor)) static void finish() {
  if (!std::getenv("IPC_ION_TRACE")) return;
  int live_handles = 0;
  int live_mappings = 0;
  for (int i = 0; i < handle_count; ++i) live_handles += handles[i].live;
  for (int i = 0; i < mapping_count; ++i) live_mappings += mappings[i].live;
  record("{\"op\":\"summary\",\"handles\":%d,\"mappings\":%d}\n", live_handles, live_mappings);
  if (trace_fd >= 0) real_close(trace_fd);
}

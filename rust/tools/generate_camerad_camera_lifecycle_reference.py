import argparse
from pathlib import Path
import subprocess
import sys

from generate_camerad_kernel_reference import extract


def replace_once(text: str, before: str, after: str) -> str:
  assert text.count(before) == 1, before
  return text.replace(before, after)


def fixture(root: Path) -> str:
  result = subprocess.run([sys.executable, str(root / 'rust/tools/generate_camerad_isp_lifecycle_reference.py'),
                           '--source', str(root), '--fixture'], check=True, capture_output=True, text=True)
  text = result.stdout
  changes = {
    '#include <media/cam_icp.h>': '#include <media/cam_icp.h>\n#include <media/cam_sensor.h>\n#include <sys/stat.h>',
    'static int tracked[2048]': 'static int camera_target = 501, camera_event = 0, camera_packet_opcode = -1;\nstatic int tracked[2048]',
    '  const char* wanted = getenv("CK_FAIL_OP");':
      '  if (setting("CK_FAIL_TARGET", 0) && setting("CK_FAIL_TARGET", 0) != camera_target) return false;\n'
      '  if (getenv("CK_FAIL_PACKET") && setting("CK_FAIL_PACKET", 0) != camera_packet_opcode) return false;\n'
      '  const char* wanted = getenv("CK_FAIL_OP");',
    '  if (!strcmp(path, "/dev/camera-fixture-isp")) fd = 503;':
      '  if (!strcmp(path, "/dev/camera-fixture-sensor")) fd = 502;\n'
      '  if (!strcmp(path, "/dev/camera-fixture-phy")) fd = 506;\n'
      '  if (!strcmp(path, "/dev/camera-fixture-isp")) fd = 503;',
    '    return syscall(SYS_openat, AT_FDCWD, path, flags, mode);':
      '    int result = syscall(SYS_openat, AT_FDCWD, path, flags, mode);\n'
      '    if (result >= 0 && strstr(path, "/dev/shm/msgq_visionbuf_") == path) {\n'
      '      if (result >= 2048 || tracked[result]) abort();\n'
      '      tracked[result] = 3; trace("{\\"op\\":\\"vision-open\\",\\"fd\\":%d}", result);\n'
      '    }\n    return result;',
    '  if (owned) {':
      '  if (fd >= 0 && fd < 2048 && tracked[fd] == 3) {\n'
      '    trace("{\\"op\\":\\"vision-mmap\\",\\"fd\\":%d,\\"length\\":%zu,\\"ok\\":%s}", fd, length, p == MAP_FAILED ? "false" : "true");\n'
      '    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; }\n'
      '  }\n  if (owned) {',
    'extern "C" int munmap(void* address, size_t length) noexcept {':
      'extern "C" int munmap(void* address, size_t length) noexcept {\n'
      '  for (int fd = 0; fd < 2048; ++fd) if (tracked[fd] == 3 && mappings[fd] == address && lengths[fd]) {\n'
      '    trace("{\\"op\\":\\"vision-munmap\\",\\"fd\\":%d,\\"length\\":%zu}", fd, length);\n'
      '    mappings[fd] = nullptr; lengths[fd] = 0; return syscall(SYS_munmap, address, length);\n'
      '  }',
    'if (fd != 501 && fd != 503 && fd != 504 && fd != 505)':
      'if (fd < 501 || fd > 506)',
    'if (acquire) reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = -1003;':
      'if (acquire) reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = 1000 + camera_target;',
    'if (packet->num_cmd_buf != 2) abort();': 'if (packet->num_cmd_buf > 2) abort();',
    '  trace("{\\"op\\":\\"target\\",\\"fd\\":%d}", fd);':
      '  camera_target = fd; camera_packet_opcode = -1;\n  trace("{\\"op\\":\\"target\\",\\"fd\\":%d}", fd);',
    'data->session_hdl = -17; data->u.frame_msg.link_hdl = -31;':
      'data->session_hdl = -1001; data->u.frame_msg.link_hdl = -1005;',
    'data->u.frame_msg.request_id = UINT64_C(0xdeadbeef01234567);':
      'data->u.frame_msg.request_id = ++camera_event;',
    'data->u.frame_msg.frame_id = UINT64_C(0xfedcba9876543210);':
      'data->u.frame_msg.frame_id = camera_event;',
    'data->u.frame_msg.timestamp = UINT64_C(0x1122334455667788);':
      'data->u.frame_msg.timestamp = UINT64_C(1000000000) + UINT64_C(50000000) * (camera_event - 1);\n'
      '      lifecycle_event(data, camera_event);',
    'data->u.frame_msg.sof_status = 7;': 'data->u.frame_msg.sof_status = 0;',
    '      if (packet->num_cmd_buf > 2) abort();':
      '      camera_packet_opcode = packet->header.op_code;\n      if (packet->num_cmd_buf > 2) abort();',
  }
  for before, after in changes.items():
    text = replace_once(text, before, after)
  helpers = (root / 'rust/tools/camerad_camera_lifecycle_fixture.cc.in').read_text()
  event_helpers, ioctl_helpers = helpers.split('// @IOCTL@\n')
  text = replace_once(text, 'static int kernel_ioctl(int fd, unsigned long request, ...) noexcept {',
                      event_helpers + '\nstatic int kernel_ioctl(int fd, unsigned long request, ...) noexcept {')
  text = replace_once(text, '    if (control->op_code == CAM_ACQUIRE_DEV) {',
                      ioctl_helpers + '\n    if (control->op_code == CAM_ACQUIRE_DEV) {')
  return text


def source(root: Path) -> str:
  source_text = (root / 'openpilot/system/camerad/cameras/spectra.cc').read_text()
  header = (root / 'openpilot/system/camerad/cameras/spectra.h').read_text()
  common = (root / 'openpilot/system/camerad/cameras/camera_common.cc').read_text()
  common_header = (root / 'openpilot/system/camerad/cameras/camera_common.h').read_text()
  util = (root / 'openpilot/common/util.h').read_text()
  macro_start = util.index('#define HANDLE_EINTR(')
  macro = util[macro_start:util.index('\n\n', macro_start)]
  helpers = [macro, extract(util, 'struct unique_fd') + ';',
             header[header.index('std::optional<int32_t> device_acquire('):header.index('class MemoryManager')],
             extract(header, 'class MemoryManager') + ';']
  helpers.extend(extract(source_text, signature) for signature in (
    'int do_cam_control(', 'int do_sync_control(', 'std::optional<int32_t> device_acquire(', 'int device_config(',
    'int device_control(', 'void *alloc_w_mmu_hdl(', 'void release(', 'void *MemoryManager::alloc_buf(',
    'void MemoryManager::free(', 'MemoryManager::~MemoryManager('))
  methods = [extract(source_text, signature) for signature in (
    'static cam_cmd_power *power_set_wait(', 'void add_patch(', 'int get_bps_blob_index(',
    'SpectraCamera::SpectraCamera(', 'SpectraCamera::~SpectraCamera(',
    'int SpectraCamera::sensors_init(', 'void SpectraCamera::sensors_i2c(', 'void SpectraCamera::sensors_poke(',
    'void SpectraCamera::sensors_start(', 'bool SpectraCamera::openSensor(',
    'void SpectraCamera::configISP(', 'void SpectraCamera::configICP(', 'void SpectraCamera::config_ife(',
    'void SpectraCamera::config_bps(', 'void SpectraCamera::configCSIPHY(', 'void SpectraCamera::linkDevices(',
    'void SpectraCamera::camera_open(', 'void SpectraCamera::camera_close(', 'void SpectraCamera::camera_map_bufs(',
    'int SpectraCamera::clear_req_queue(', 'void SpectraCamera::enqueue_frame(', 'void SpectraCamera::destroySyncObjectAt(',
    'bool SpectraCamera::handle_camera_event(', 'bool SpectraCamera::validateEvent(', 'void SpectraCamera::clearAndRequeue(',
    'bool SpectraCamera::waitForFrameReady(', 'bool SpectraCamera::processFrame(', 'bool SpectraCamera::syncFirstFrame(')]
  constants = header[header.index('enum {'):header.index('\n};', header.index('enum {')) + 3]
  ife = (root / 'openpilot/system/camerad/cameras/ife.h').read_text()
  replacements = {
    '@CONSTANTS@': constants,
    '@HELPERS@': '\n\n'.join(helpers),
    '@MASTER@': extract(header, 'class SpectraMaster') + ';',
    '@BUFFER@': extract(header, 'class SpectraBuf') + ';',
    '@IMAGE@': extract(common_header, 'typedef struct FrameMetadata') + ' FrameMetadata;\n' + extract(common_header, 'class CameraBuf {') + ';',
    '@CAMERA@': extract(header, 'class SpectraCamera {') + ';',
    '@IFE@': ife[ife.index('int build_common_ife_bps('):],
    '@METHODS@': '\n\n'.join(methods),
    '@IMAGE_METHODS@': '\n\n'.join(extract(common, signature) for signature in ('void CameraBuf::init(', 'CameraBuf::~CameraBuf(')),
  }
  template = (root / 'rust/tools/camerad_camera_lifecycle_source.cc.in').read_text()
  for key, value in replacements.items():
    template = replace_once(template, key, value)
  return template


def main() -> None:
  parser = argparse.ArgumentParser(description='Extract original complete camera lifecycle with real Vision allocation')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--fixture', action='store_true')
  arguments = parser.parse_args()
  print(fixture(arguments.source) if arguments.fixture else source(arguments.source))


if __name__ == '__main__':
  main()

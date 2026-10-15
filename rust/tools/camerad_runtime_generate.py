import argparse
from pathlib import Path

from generate_camerad_camera_lifecycle_reference import fixture, replace_once


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  args = parser.parse_args()
  text = fixture(args.source)
  helpers = (args.source / 'rust/tools/camerad_runtime_fixture.cc.in').read_text()
  text = replace_once(
    text, 'static int opened(const char* path, int flags, mode_t mode) {', helpers + '\nstatic int opened(const char* path, int flags, mode_t mode) {'
  )
  text = replace_once(
    text,
    '  if (fd < 0) {',
    '  int actual = runtime_open(path, flags);\n'
    + '  if (actual == -2) return -1;\n  if (actual < -100) return -actual-100;\n'
    + '  if (actual >= 0) fd = actual;\n  if (fd < 0) {',
  )
  start = text.index('extern "C" int poll(')
  end = text.index('\nstatic ', start)
  text = text[:start] + '''extern "C" int poll(struct pollfd* fds, nfds_t count, int timeout) {
    if (count && fds[0].fd == 501) return runtime_poll(fds, count, timeout);
    timespec delay{timeout / 1000, (timeout % 1000) * 1000000L};
    return syscall(SYS_ppoll, fds, count, timeout < 0 ? nullptr : &delay, nullptr, 0);
  }
''' + text[end:]
  start = text.index('      data->session_hdl = -1001;')
  end = text.index('      data->u.frame_msg.sof_status = 0;', start) + len('      data->u.frame_msg.sof_status = 0;')
  text = text[:start] + '      runtime_event(data);' + text[end:]
  text = replace_once(
    text,
    'reinterpret_cast<cam_req_mgr_session_info*>(data)->session_hdl = -1001;',
    'reinterpret_cast<cam_req_mgr_session_info*>(data)->session_hdl = 10001 + runtime_port;',
  )
  text = replace_once(
    text,
    'reinterpret_cast<cam_req_mgr_link_info*>(data)->link_hdl = -1005;',
    'reinterpret_cast<cam_req_mgr_link_info*>(data)->link_hdl = 20001 + runtime_port;',
  )
  text = replace_once(
    text,
    'reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = 1000 + camera_target;',
    'reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = 100000 + runtime_port * 1000 + camera_target;',
  )
  text = replace_once(text, 'if (fd < 501 || fd > 506)', 'if (fd < 501 || fd > 522)')
  text = replace_once(
    text,
    '      int ret = expected && probe->expected_data != expected ? -1 : 0;',
    '      int ret = (setting("CK_MISSING_PORT", -1) == runtime_port || (expected && probe->expected_data != expected)) ? -1 : 0;',
  )
  text = replace_once(
    text,
    '    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; }\n  }\n  if (owned)',
    '    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; memset(p, 64 + runtime_port * 32, length); }\n  }\n  if (owned)',
  )
  marker = '  camera_target = fd; camera_packet_opcode = -1;'
  queries = '''
  if (fd == 501 && request == VIDIOC_CAM_CONTROL) {
    auto* control = static_cast<cam_control*>(payload);
    if (control->op_code == CAM_REQ_MGR_SCHED_REQ) {
      auto* schedule = reinterpret_cast<cam_req_mgr_sched_request*>(control->handle);
      int port = schedule->session_hdl - 10001;
      if (port >= 0 && port < 3 && schedule->req_id > 18) runtime_completed[port] = schedule->req_id - 18;
    }
  }
  if (request == VIDIOC_SUBSCRIBE_EVENT) {
    trace("{\\"op\\":\\"subscribe\\",\\"fd\\":%d}", fd);
    return 0;
  }
  if (request == VIDIOC_CAM_CONTROL) {
    auto* control = static_cast<cam_control*>(payload);
    if (control->op_code == CAM_QUERY_CAP) {
      auto* query = reinterpret_cast<cam_query_cap_cmd*>(control->handle);
      if (fd == 503) {
        auto* caps = reinterpret_cast<cam_isp_query_cap_cmd*>(query->caps_handle);
        caps->device_iommu.non_secure = 0x102; caps->cdm_iommu.non_secure = 0x304;
      } else if (fd == 504) {
        auto* caps = reinterpret_cast<cam_icp_query_cap_cmd*>(query->caps_handle);
        caps->dev_iommu_handle.non_secure = 0x506;
      } else abort();
      trace("{\\"op\\":\\"query\\",\\"fd\\":%d}", fd);
      return 0;
    }
  }
'''
  text = replace_once(text, marker, queries + '\n' + marker)
  marker = (
    '      for (uint32_t index = 0; index < packet->num_cmd_buf; ++index) '
    + 'snapshot(descriptors[index].mem_handle, descriptors[index].offset, descriptors[index].length);'
  )
  registers = '''
      if (fd >= 510 && fd <= 512 && packet->num_cmd_buf == 1 && runtime_round > 0) {
        const auto* bytes = static_cast<const uint8_t*>(mappings[descriptors[0].mem_handle >> 16]) + descriptors[0].offset;
        const auto* writes = reinterpret_cast<const cam_cmd_i2c_random_wr*>(bytes);
        if (descriptors[0].length >= sizeof(i2c_rdwr_header) && writes->header.count < 64) {
          char payload[4096];
          hex(writes->random_wr_payload, writes->header.count * sizeof(i2c_random_wr_payload), payload);
          trace("{\\"op\\":\\"runtime-registers\\",\\"port\\":%d,\\"round\\":%d,\\"request\\":%lld,\\"count\\":%u,\\"payload\\":\\"%s\\"}",
                fd - 510, runtime_round, static_cast<long long>(runtime_completed[fd - 510]), writes->header.count, payload);
          static bool barrier_used = false;
          if (const char* gate = getenv("CK_REGISTER_GATE"); gate && !barrier_used && fd == 510) {
            barrier_used = true;
            char ready[4096]; snprintf(ready, sizeof(ready), "%s.ready", gate);
            int signal_fd = syscall(SYS_openat, AT_FDCWD, ready, O_WRONLY | O_CREAT, 0600);
            if (signal_fd < 0) abort();
            syscall(SYS_close, signal_fd);
            while (syscall(SYS_faccessat, AT_FDCWD, gate, F_OK) != 0) runtime_pause();
            trace("{\\"op\\":\\"runtime-register-barrier-released\\",\\"port\\":0,\\"round\\":%d}", runtime_round);
          }
        }
      }
'''
  assert text.count(marker) == 2
  before, matched, after = text.rpartition(marker)
  text = before + matched + '\n' + registers + after
  text = '#include <atomic>\n#include <string>\n#include <sys/stat.h>\n' + text
  text = text.replace('static int camera_target = 501, camera_event = 0, camera_packet_opcode = -1;',
                      'static thread_local int camera_target = 501, camera_packet_opcode = -1; static int camera_event = 0;')
  text = text.replace('syncs = 0, failures = 0;', 'syncs = 0; static std::atomic<int> failures{0};')
  text = text.replace('syscall(SYS_write, log_fd, bytes, n); syscall(SYS_write, log_fd, "\\n", 1);',
                      "bytes[n++] = '\\n'; syscall(SYS_write, log_fd, bytes, n);")
  # OwnedFd duplicates share the inert sync device's inode. Preserve real FD
  # ownership while mapping only this fixture's duplicate back to its ABI peer.
  text = replace_once(text, '  if (fd < 501 || fd > 522)', '''  if (fd < 501 || fd > 522) {
    struct stat actual{}, sync{};
    if (!fstat(fd, &actual) && !fstat(505, &sync) && actual.st_dev == sync.st_dev && actual.st_ino == sync.st_ino) fd = 505;
  }
  if (fd < 501 || fd > 522)''')
  marker = '  camera_target = fd; camera_packet_opcode = -1;'
  text = replace_once(text, marker, '''  if (fd == 505 && request == CAM_PRIVATE_IOCTL_CMD && syscall(SYS_gettid) != getpid()) {
    auto* arg = static_cast<cam_private_ioctl_arg*>(payload);
    static thread_local bool waited = false;
    if (const char* gate = getenv("CK_DRIVER_WAIT_GATE"); gate && arg->id == CAM_SYNC_WAIT && !waited) {
      waited = true;
      std::string ready = std::string(gate) + ".ready";
      int mark = syscall(SYS_openat, AT_FDCWD, ready.c_str(), O_WRONLY | O_CREAT, 0600);
      if (mark < 0) abort();
      syscall(SYS_close, mark);
      trace("{\\"op\\":\\"driver-wait-blocked\\"}");
      while (syscall(SYS_faccessat, AT_FDCWD, gate, F_OK) != 0) runtime_pause();
      trace("{\\"op\\":\\"driver-wait-released\\"}");
    }
  }
''' + marker)
  print(text)


if __name__ == '__main__':
  main()

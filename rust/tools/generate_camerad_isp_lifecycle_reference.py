import argparse
from pathlib import Path

from generate_camerad_kernel_reference import extract


def main() -> None:
  parser = argparse.ArgumentParser(description="Emit original ISP/BPS allocation and configuration paths")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--fixture", action="store_true")
  args = parser.parse_args()
  if args.fixture:
    fixture = (args.source / "rust/tools/camerad_kernel_fixture.cc").read_text()
    changes = {
      'extern "C" int ioctl(int fd, unsigned long request, ...) noexcept {': 'static int kernel_ioctl(int fd, unsigned long request, ...) noexcept {',
      '  if (fd < 0) {': (
        '  if (!strcmp(path, "/dev/camera-fixture-isp")) fd = 503;\n'
        + '  if (!strcmp(path, "/dev/camera-fixture-icp")) fd = 504;\n'
        + '  if (!strcmp(path, "/dev/camera-fixture-sync")) fd = 505;\n  if (fd < 0) {'
      ),
      (
        '    if (length > 4096) abort();\n    char data[8193]; hex(address, length, data);\n'
        + '    trace("{\\"op\\":\\"munmap\\",\\"fd\\":%d,\\"length\\":%zu,\\"data\\":\\"%s\\"}", fd, length, data);'
      ): ('    dump_memory("release", fd, address, 0, length);\n    trace("{\\"op\\":\\"munmap\\",\\"fd\\":%d,\\"length\\":%zu}", fd, length);'),
      ('      if (command->len > sizeof(bytes) || syscall(SYS_pwrite64, export_fd, bytes, command->len, 0) != static_cast<ssize_t>(command->len)) abort();'): (
        '      for (size_t offset = 0; offset < command->len; offset += sizeof(bytes)) {\n'
        + '        size_t count = std::min(sizeof(bytes), static_cast<size_t>(command->len) - offset);\n'
        + '        if (syscall(SYS_pwrite64, export_fd, bytes, count, offset) != static_cast<ssize_t>(count)) abort();\n      }'
      ),
    }
    for old, new in changes.items():
      assert fixture.count(old) == 1, old
      fixture = fixture.replace(old, new)
    prelude, wrapper = (args.source / "rust/tools/camerad_isp_lifecycle_fixture.cc.in").read_text().split("// @WRAPPER@\n")
    fixture = fixture.replace('extern "C" int munmap(', prelude + '\nextern "C" int munmap(', 1)
    print('#include <algorithm>\n#include <cstddef>\n#include <cstdint>\n#include <media/cam_isp.h>\n#include <media/cam_icp.h>\n' + fixture + wrapper)
    return
  source = (args.source / "openpilot/system/camerad/cameras/spectra.cc").read_text()
  header = (args.source / "openpilot/system/camerad/cameras/spectra.h").read_text()
  util = (args.source / "openpilot/common/util.h").read_text()
  start = util.index("#define HANDLE_EINTR(")
  macro = util[start : util.index("\n\n", start)]
  declarations = header[header.index("std::optional<int32_t> device_acquire(") : header.index("class MemoryManager")]
  helpers = [macro, extract(util, "struct unique_fd") + ";", declarations, extract(header, "class MemoryManager") + ";"]
  helpers += [
    extract(source, signature)
    for signature in (
      "int do_cam_control(",
      "std::optional<int32_t> device_acquire(",
      "int device_config(",
      "int device_control(",
      "void *alloc_w_mmu_hdl(",
      "void release(",
      "void *MemoryManager::alloc_buf(",
      "void MemoryManager::free(",
      "MemoryManager::~MemoryManager(",
    )
  ]
  methods = [
    extract(source, signature)
    for signature in (
      "void add_patch(",
      "int get_bps_blob_index(",
      "void SpectraCamera::configISP(",
      "void SpectraCamera::configICP(",
      "void SpectraCamera::config_ife(",
      "void SpectraCamera::config_bps(",
    )
  ]
  ife = (args.source / "openpilot/system/camerad/cameras/ife.h").read_text()
  template = (args.source / "rust/tools/camerad_isp_lifecycle_source.cc.in").read_text()
  print(
    template.replace("@HELPERS@", "\n\n".join(helpers))
    .replace("@BUFFER@", extract(header, "class SpectraBuf") + ";")
    .replace("@IFE@", ife[ife.index("int build_common_ife_bps(") :])
    .replace("@METHODS@", "\n\n".join(methods))
    .rstrip()
  )


if __name__ == "__main__":
  main()

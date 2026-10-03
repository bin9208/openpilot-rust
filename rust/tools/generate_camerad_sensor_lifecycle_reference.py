import argparse
from pathlib import Path

from generate_camerad_kernel_reference import extract


def main() -> None:
  parser = argparse.ArgumentParser(description="Emit original sensor lifecycle with real kernel-boundary fixtures")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--fixture", action="store_true")
  args = parser.parse_args()
  if args.fixture:
    fixture = (args.source / "rust/tools/camerad_kernel_fixture.cc").read_text()
    changes = {
      "extern \"C\" int ioctl(int fd, unsigned long request, ...) noexcept {": "static int kernel_ioctl(int fd, unsigned long request, ...) noexcept {",
      "  if (fd < 0) {": '  if (!strcmp(path, "/dev/camera-fixture-sensor")) fd = 502;\n  if (fd < 0) {',
      "char bytes[16384]": "char bytes[262144]",
      "uint8_t bytes[4096]": "uint8_t bytes[65536]",
      "if (length > 4096)": "if (length > 65536)",
      "char data[8193]": "char data[131073]",
    }
    for old, new in changes.items():
      assert fixture.count(old) == 1, old
      fixture = fixture.replace(old, new)
    print(fixture + (args.source / "rust/tools/camerad_sensor_lifecycle_fixture.cc.in").read_text())
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
      "static cam_cmd_power *power_set_wait(",
      "int SpectraCamera::sensors_init(",
      "void SpectraCamera::sensors_i2c(",
      "void SpectraCamera::sensors_poke(",
      "void SpectraCamera::sensors_start(",
      "bool SpectraCamera::openSensor(",
    )
  ]
  constants = header[header.index("enum {") : header.index("\n};", header.index("enum {")) + 3]
  template = (args.source / "rust/tools/camerad_sensor_lifecycle_source.cc.in").read_text()
  print(template.replace("@CONSTANTS@", constants).replace("@HELPERS@", "\n\n".join(helpers)).replace("@METHODS@", "\n\n".join(methods)).rstrip())


if __name__ == "__main__":
  main()

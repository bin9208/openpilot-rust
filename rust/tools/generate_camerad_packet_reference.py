import argparse
from pathlib import Path


def main() -> None:
  parser = argparse.ArgumentParser(description="Emit unchanged Spectra sensor packet builders with an in-memory ioctl boundary")
  parser.add_argument("--source", type=Path, required=True)
  args = parser.parse_args()
  source = (args.source / "openpilot/system/camerad/cameras/spectra.cc").read_text()
  methods = []
  for signature in [
    "static cam_cmd_power *power_set_wait(",
    "void SpectraCamera::sensors_poke(",
    "void SpectraCamera::sensors_i2c(",
    "int SpectraCamera::sensors_init(",
    "void SpectraCamera::configCSIPHY(",
    "void add_patch(",
    "void SpectraCamera::config_ife(",
    "void SpectraCamera::config_bps(",
    "int get_bps_blob_index(",
  ]:
    start = source.index(signature)
    stop = source.index("\n}", start) + 2
    if source[stop : stop + 1] == ";":
      stop += 1
    methods.append(source[start:stop])
  template = (args.source / "rust/tools/camerad_packets_source.cc.in").read_text()
  ife = (args.source / "openpilot/system/camerad/cameras/ife.h").read_text()
  ife = ife[ife.index("int build_common_ife_bps(") :]
  start = source.index("  uint32_t bl = sensor->black_level")
  stop = source.index("\n\n  assert(sensor->gamma_lut_rgb", start)
  linearization = source[start:stop]
  start = source.index("  struct cam_isp_in_port_info in_port_info =")
  stop = source.index("\n\n  struct cam_isp_resource", start)
  port = source[start:stop]
  start = source.index("  struct cam_icp_acquire_dev_info icp_info =")
  stop = source.index("\n  auto h = device_acquire", start)
  resource = source[start:stop]
  rendered = template.replace("@IFE@", ife).replace("@LINEARIZATION@", linearization).replace("@ACQUIRE_PORT@", port)
  print(rendered.replace("@ACQUIRE_BPS@", resource).replace("@METHODS@", "\n\n".join(methods)).rstrip())


if __name__ == "__main__":
  main()

import argparse
from pathlib import Path
from typing import cast


def main() -> None:
  parser = argparse.ArgumentParser(description="Emit a test adapter with original, verbatim camera exposure methods")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--kind", choices=("ae", "misc"), default="ae")
  args = parser.parse_args()
  root = cast(Path, args.source)
  if args.kind == "misc":
    source = (root / "openpilot/system/camerad/cameras/camera_common.cc").read_text()
    start = source.index("float calculate_exposure_value(")
    stop = source.index("\n}", start) + 2
    template = (root / "rust/tools/camerad_misc_source.cc.in").read_text()
    print(template.replace("@LUMINANCE@", source[start:stop]).rstrip())
    return
  source = (root / "openpilot/system/camerad/cameras/camera_qcom2.cc").read_text()
  template = (root / "rust/tools/camerad_ae_source.cc.in").read_text()
  methods = []
  for name in ["set_exposure_rect", "update_exposure_score", "set_camera_exposure"]:
    start = source.index(f"void CameraState::{name}(")
    stop = source.index("\n}", start) + 2
    methods.append(source[start:stop])
  start = source.index("float get_gain_factor() const {")
  stop = source.index("\n  }", start) + 4
  template = template.replace("@GAIN_FACTOR@", source[start:stop])
  start = source.index("  fl_pix = camera.cc.focal_len")
  stop = source.index("\n\n  pm =", start)
  initialization = source[start:stop]
  print(template.replace("@METHODS@", "\n\n".join(methods)).replace("@INITIALIZE@", initialization).rstrip())


if __name__ == "__main__":
  main()

import argparse
from pathlib import Path


def main() -> None:
  parser = argparse.ArgumentParser(description="Emit unchanged Spectra frame lifecycle methods with controlled I/O boundaries")
  parser.add_argument("--source", type=Path, required=True)
  args = parser.parse_args()
  source = (args.source / "openpilot/system/camerad/cameras/spectra.cc").read_text()
  methods = []
  for name, result in [
    ("handle_camera_event", "bool"),
    ("validateEvent", "bool"),
    ("clearAndRequeue", "void"),
    ("waitForFrameReady", "bool"),
    ("processFrame", "bool"),
    ("syncFirstFrame", "bool"),
  ]:
    start = source.index(f"{result} SpectraCamera::{name}(")
    stop = source.index("\n}", start) + 2
    methods.append(source[start:stop])
  template = (args.source / "rust/tools/camerad_requests_source.cc.in").read_text()
  print(template.replace("@METHODS@", "\n\n".join(methods)).rstrip())


if __name__ == "__main__":
  main()

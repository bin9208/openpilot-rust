import argparse
from pathlib import Path
import subprocess


def main() -> None:
  parser = argparse.ArgumentParser(description="Emit unchanged Spectra frame lifecycle methods with controlled I/O boundaries")
  parser.add_argument("--source", type=Path, required=True)
  args = parser.parse_args()
  source = subprocess.check_output(['git', 'show', 'e8e152dbcfd9574a1791ed12026d1880e87689b3:openpilot/system/camerad/cameras/spectra.cc'], cwd=args.source, text=True)
  methods = []
  for name, result in [
    ("handle_camera_event", "bool"),
    ("validateEvent", "bool"),
    ("clearAndRequeue", "void"),
    ("frameWaitTask", "std::function<bool()>"),
    ("finishFrame", "bool"),
    ("complete_frame_wait", "bool"),
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

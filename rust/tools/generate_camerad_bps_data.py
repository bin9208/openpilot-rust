import argparse
import hashlib
import re
from pathlib import Path


def main() -> None:
  parser = argparse.ArgumentParser(description="Convert the original generated BPS byte arrays to Rust, including C zero padding")
  parser.add_argument("--source", type=Path, required=True)
  args = parser.parse_args()
  path = args.source / "openpilot/system/camerad/cameras/bps_blobs.h"
  source = path.read_text()
  print("// Generated from openpilot/system/camerad/cameras/bps_blobs.h; original project MIT license applies.")
  print(f"// Source SHA-256: {hashlib.sha256(path.read_bytes()).hexdigest()}")
  print("// Regenerate with rust/tools/generate_camerad_bps_data.py --source <checkout>.\n")
  arrays = re.findall(r"unsigned char (\w+)\[(\d+)\]\[([^]]+)\] = \{(.*?)\n\};", source, re.S)
  assert len(arrays) == 3
  for name, count, width, body in arrays:
    count, width = int(count), int(width, 0)
    rows = re.findall(r"\{([^{}]*)\}", body)
    assert len(rows) == count
    print(f"pub const {name.upper()}: [[u8; {width}]; {count}] = [")
    for row in rows:
      row = re.sub(r"/\*.*?\*/", "", row, flags=re.S)
      values = [int(value.strip(), 0) for value in row.split(",") if value.strip()]
      assert len(values) <= width and all(0 <= value <= 255 for value in values)
      values.extend([0] * (width - len(values)))
      if not any(values):
        print(f"    [0; {width}],")
        continue
      print("    [")
      for start in range(0, width, 15):
        print("        " + ", ".join(f"0x{value:02x}" for value in values[start : start + 15]) + ",")
      print("    ],")
    print("];\n")


if __name__ == "__main__":
  main()

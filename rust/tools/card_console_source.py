from __future__ import annotations

import hashlib
import json
from pathlib import Path
import unicodedata


def main() -> None:
  assert unicodedata.unidata_version == "15.0.0", unicodedata.unidata_version
  ranges: list[tuple[int, int]] = []
  first: int | None = None
  for point in range(0x110000):
    if not chr(point).isprintable():
      if first is None:
        first = point
    elif first is not None:
      ranges.append((first, point - 1))
      first = None
  if first is not None:
    ranges.append((first, 0x10ffff))
  rows = [f"(0x{start:x}, 0x{end:x})" for start, end in ranges]
  rust = "pub(super) const NON_PRINTABLE: &[(u32, u32)] = &[\n" + "".join(f"    {row},\n" for row in rows) + "];\n"
  path = Path("rust/crates/card/src/identification/printable_ranges.rs")
  path.write_text(rust)
  provenance = {"unicode_version": unicodedata.unidata_version, "python_version": __import__("sys").version,
                "algorithm": "inclusive maximal Unicode ranges where Python str.isprintable is false", "range_count": len(ranges),
                "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "table_sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
  path.with_suffix(".json").write_text(json.dumps(provenance, indent=2) + "\n")


if __name__ == "__main__":
  main()

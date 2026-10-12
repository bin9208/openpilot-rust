from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import numpy as np
from PIL import Image


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("reference", type=Path)
  parser.add_argument("binary", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--case", action="append", dest="selected_cases")
  args = parser.parse_args()
  root, reference, binary, output = (value.resolve() for value in (args.root, args.reference, args.binary, args.output))
  baseline = json.loads((reference / "source-result.json").read_text())
  source = root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py"
  assert hashlib.sha256(source.read_bytes()).hexdigest() == baseline["source_sha256"], "source changed since capture"
  output.mkdir(parents=True, exist_ok=False)
  input_path = reference / "input.json"
  if args.selected_cases:
    request = json.loads(input_path.read_text())
    wanted = set(args.selected_cases)
    assert wanted <= {case["name"] for case in request["cases"]}, "unknown selected case"
    request["cases"] = [case for case in request["cases"] if case["name"] in wanted]
    baseline["captures"] = [row for row in baseline["captures"] if row["name"] in wanted]
    baseline["cases"] = len(request["cases"])
    input_path = output / "selected-input.json"
    input_path.write_text(json.dumps(request))
  command = [str(binary), str(root), str(input_path), str(output)]
  binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
  (output / "command.json").write_text(json.dumps(command))
  with (output / "native.log").open("wb") as log:
    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
  native = json.loads((output / "native-result.json").read_text())
  assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_hash, "provider changed during capture"
  source_captures = {(row["name"], row["warm"]): row for row in baseline["captures"]}
  native_captures = {(row["name"], row["warm"]): row for row in native["captures"]}
  assert len(source_captures) == len(baseline["captures"]), "duplicate original frame"
  assert len(native_captures) == len(native["captures"]), "duplicate native frame"
  assert source_captures.keys() == native_captures.keys(), "frame set mismatch"
  results = []
  for key, source_row in source_captures.items():
    native_row = native_captures[key]
    expected_path, actual_path = Path(source_row["path"]), Path(native_row["path"])
    assert hashlib.sha256(expected_path.read_bytes()).hexdigest() == source_row["sha256"], expected_path
    with Image.open(expected_path) as expected_image, Image.open(actual_path) as actual_image:
      assert expected_image.format == actual_image.format == "PNG", key
      dimensions = (source_row["width"], source_row["height"])
      assert expected_image.size == actual_image.size == dimensions, (key, expected_image.size, actual_image.size)
      expected = np.asarray(expected_image.convert("RGBA"), dtype=np.int16)
      actual = np.asarray(actual_image.convert("RGBA"), dtype=np.int16)
    difference = np.abs(expected - actual)
    changed = np.any(difference != 0, axis=2)
    y, x = np.nonzero(changed)
    results.append(
      {
        "name": key[0],
        "warm": key[1],
        "width": dimensions[0],
        "height": dimensions[1],
        "different_pixels": int(changed.sum()),
        "maximum_channel_error": int(difference.max()),
        "different_alpha": int(np.count_nonzero(difference[:, :, 3])),
        "bounds": [int(x.min()), int(y.min()), int(x.max()) + 1, int(y.max()) + 1] if len(x) else None,
        "native_sha256": hashlib.sha256(actual_path.read_bytes()).hexdigest(),
      }
    )
  failures = [row for row in results if row["different_pixels"]]
  rust_files = sorted((root / "rust/crates/cluster/src/renderer").rglob("*.rs"))
  receipt = {
    "cases": baseline["cases"],
    "frames": len(results),
    "failed_frames": len(failures),
    "binary_sha256": binary_hash,
    "source_sha256": baseline["source_sha256"],
    "worktree_sources_at_comparison": {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest() for path in rust_files},
    "results": results,
  }
  (output / "result.json").write_text(json.dumps(receipt, indent=2))
  print(json.dumps({"frames": len(results), "failed_frames": len(failures), "first_failures": failures[:5]}))
  if failures:
    raise SystemExit(1)


if __name__ == "__main__":
  main()

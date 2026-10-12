from __future__ import annotations

import argparse
import ast
import base64
import hashlib
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace


def source_function(path: Path, name: str):
  tree = ast.parse(path.read_text())
  node = next(item for item in ast.walk(tree) if isinstance(item, ast.FunctionDef) and item.name == name)
  node.decorator_list = []
  namespace = {}
  exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), "exec"), namespace)
  return namespace[name]


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path, required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  source_root = args.root / "openpilot/selfdrive/carrot/cluster"
  paths = [source_root / "main.py", source_root / "cluster_h264_pipeline.py"]
  rgba = source_function(paths[0], "build_rgba_color_test_pattern")
  nv12 = source_function(paths[1], "build_nv12_color_test_pattern")
  bitrate = source_function(paths[1], "_parse_bitrate_bps")
  requests, expected = [], []
  for width, height in [(w, h) for w in [1, 2, 3, 6, 17, 32] for h in [1, 2, 3, 6, 17, 32]] + [(480, 800), (1280, 720)]:
    requests.append({"kind": "rgba", "width": width, "height": height})
    expected.append({"bytes": base64.b64encode(rgba(width, height)).decode()})
  for width, height in [(w, h) for w in [2, 4, 6, 10, 16, 30] for h in [2, 4, 6, 10, 16, 30]] + [(480, 800), (1280, 720)]:
    for padding in [0, 16]:
      stride, y_rows, uv_rows = width + padding, height + padding, height // 2 + padding
      uv_offset = stride * y_rows + padding
      byte_count = uv_offset + stride * uv_rows + padding
      layout = {"width": width, "height": height, "stride": stride, "y_scanlines": y_rows,
                "uv_scanlines": uv_rows, "uv_offset": uv_offset, "byte_count": byte_count}
      source = SimpleNamespace(encoder_width=width, encoder_height=height, _native_input_stride=stride,
                               _native_input_bytesused=byte_count, _native_input_uv_offset=uv_offset,
                               _native_input_uv_scanlines=uv_rows)
      requests.append({"kind": "nv12", "layout": layout})
      expected.append({"bytes": base64.b64encode(nv12(source)).decode()})
  texts = ["", " ", "0", "-1", "0.49", "0.5", "0.50001", "1.49", "1.5", "1.999", "4m", "4M", "250k", "250K",
           "1e3", "1_000", "1_0k", "١٢٣", "１２３", "1_", "_1", "1__0", "nan", "inf", "-inf", "0x10", "1.1.1", "2 m",
           "1e-9", "2.147483646e9", "2147483647", "2147483647.49", "+42", "-0.0", ".5k", "1e-3m", "\x00"]
  spaces = [chr(point) for point in range(0x110000) if chr(point).isspace()]
  texts += [space + text + space for space in spaces for text in ["4m", "1.5", "0.5k"]]
  for text in texts:
    requests.append({"kind": "bitrate", "text": text})
    try:
      value = bitrate(text)
      expected.append({"value": value})
    except (ValueError, OverflowError):
      expected.append({"error": True})
  (args.output / "requests.json").write_text(json.dumps(requests))
  (args.output / "source.json").write_text(json.dumps(expected))
  run = subprocess.run([str(args.native)], input=json.dumps(requests), capture_output=True, text=True, timeout=30, check=True)
  (args.output / "native.json").write_text(run.stdout)
  actual = json.loads(run.stdout)
  failures = [{"index": i, "request": request, "source": left if "bytes" not in left else {"byte_count": len(base64.b64decode(left["bytes"]))},
               "native": right if "bytes" not in right else {"byte_count": len(base64.b64decode(right["bytes"]))}}
              for i, (request, left, right) in enumerate(zip(requests, expected, actual, strict=True)) if left != right]
  report = {"cases": len(requests), "rgba_cases": 38, "nv12_cases": 76, "bitrate_cases": len(texts),
            "failures": failures, "source_sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths},
            "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
            "scope": "Original function bodies; positive RGBA, valid even encoder layouts with padding; bitrate within signed C int ABI; no device"}
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))
  if failures:
    raise AssertionError(failures)


if __name__ == "__main__":
  main()

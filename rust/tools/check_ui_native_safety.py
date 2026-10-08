#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_native_safety.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil

from check_startup_ui import Context, adapter_asan


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  parser.add_argument('--raylib-root', type=Path, required=True)
  parser.add_argument('--raylib-library', type=Path, required=True)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  free = shutil.disk_usage(root).free
  assert free > (25 + 0.25) * 1024**3, free
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  environment = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1', STARTUP_UI_RAYLIB_LIBRARY=str(args.raylib_library.resolve()))
  ctx = Context(root, args.target.resolve(), output, args.raylib_root.resolve(), environment, 'g++', '1.94.0')
  adapter_asan(ctx)
  headers = list(ctx.target.glob('build/openpilot-ui-application-*/out/cxxbridge/include/openpilot-ui-application/src/params/numeric/bridge.rs.h'))
  assert headers, 'UI CXX bridge must be built first'
  header = max(headers, key=lambda path: path.stat().st_mtime).parents[4]
  executable = output / 'params-numeric-asan'
  ctx.run(
    'params-asan-build',
    [
      'g++',
      '-std=c++17',
      '-g',
      '-fsanitize=address,undefined',
      '-fno-omit-frame-pointer',
      f'-I{header}',
      f'-I{root / "rust/crates/ui-application/native"}',
      str(root / 'rust/tools/ui_application_qa/params_numeric_asan.cc'),
      str(root / 'rust/crates/ui-application/native/params_numeric.cc'),
      str(output / 'libcxx_runtime.a'),
      '-ldl',
      '-lpthread',
      '-o',
      str(executable),
    ],
  )
  ctx.run('params-asan', [str(executable)], {'ASAN_OPTIONS': 'detect_leaks=0:halt_on_error=1', 'UBSAN_OPTIONS': 'halt_on_error=1'})
  hashes = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in [output / 'bridge-asan', executable]}
  (output / 'result.json').write_text(
    json.dumps(
      {
        'verdict': 'PASS',
        'native_bridge_cycles': 3,
        'borrowed_parser_cycles': 1000,
        'large_input_bytes': 1048579,
        'binary_sha256': hashes,
        'limits': 'external prebuilt raylib and Rust CXX runtime uninstrumented; graphics driver leak checks disabled',
      },
      indent=2,
    )
  )
  print('PASS native bridge and Params parser ASAN/UBSAN')


if __name__ == '__main__':
  main()

"""AddressSanitizer coverage for the two added source drawing primitives."""

import argparse
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--target', type=Path, required=True)
parser.add_argument('--raylib', type=Path, required=True)
parser.add_argument('--evidence', type=Path, required=True)
args = parser.parse_args()
root = Path.cwd()
args.evidence.mkdir(parents=True, exist_ok=True)
headers = list(args.target.glob('build/openpilot-startup-ui-*/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h'))
header = max(headers, key=lambda p: p.stat().st_mtime).parents[2]
cxx = max(args.target.glob('deps/libcxx-*.rlib'), key=lambda p: p.stat().st_mtime)
runtime = args.evidence / 'libcxx_runtime.a'
program = args.evidence / 'model-adapter-asan'
commands = [
  [
    'rustup',
    'run',
    '1.94.0',
    'rustc',
    '--edition=2024',
    '--crate-type=staticlib',
    str(root / 'rust/tools/startup_ui_qa/cxx_runtime.rs'),
    '--extern',
    f'cxx={cxx}',
    '-L',
    f'dependency={args.target / "deps"}',
    '-o',
    str(runtime),
  ],
  [
    'clang++',
    '-std=c++17',
    '-g',
    '-fsanitize=address,undefined',
    '-fno-omit-frame-pointer',
    f'-I{header}',
    f'-I{root / "rust/crates/startup-ui/native"}',
    f'-I{args.raylib / "include"}',
    str(root / 'rust/tools/ui_model_qa/bridge_asan.cc'),
    *[str(root / 'rust/crates/startup-ui/native' / name) for name in ['bridge.cc', 'raylib_loader.cc', 'graphics.cc', 'egl.cc']],
    str(runtime),
    '-ldl',
    '-lpthread',
    '-o',
    str(program),
  ],
  [str(program), str(args.evidence / 'capture.png')],
]
for index, command in enumerate(commands):
  with (args.evidence / f'{index}.log').open('w') as log:
    subprocess.run(
      command,
      check=True,
      stdout=log,
      stderr=subprocess.STDOUT,
      env=dict(os.environ, ASAN_OPTIONS='detect_leaks=0:halt_on_error=1', UBSAN_OPTIONS='halt_on_error=1'),
    )
(args.evidence / 'results.json').write_text(
  json.dumps(
    {
      'passed': True,
      'invocations': commands,
      'observable': 'zero exit; ASAN/UBSAN halt-on-error; nonempty real-GL capture',
      'capture_bytes': (args.evidence / 'capture.png').stat().st_size,
    },
    indent=2,
  )
)

#!/usr/bin/env python3
"""Stage the uv.lock-pinned native raylib archive without a Python UI runtime."""
import argparse
import hashlib
import io
import json
import subprocess
from pathlib import Path
import urllib.request
import zipfile

WHEELS = {
  'x86_64': (
    'https://files.pythonhosted.org/packages/27/ce/e8b941bcf0b213ca9cb4d8cb69b18398b4dad77923925d80bc3281837461/'
    'comma_deps_raylib-6.0.0.1.post103-py3-none-manylinux_2_28_x86_64.whl',
    '942ec942ec01005f05584eede1a6add483043e8e8f764e68cfebb7c21c443b89', 'libraylib_desktop.a'),
  'aarch64': (
    'https://files.pythonhosted.org/packages/3d/4d/cceaf969d771af8870094d06bb84dca805f9268584a9a3c134dfee2307c7/'
    'comma_deps_raylib-6.0.0.1.post103-py3-none-manylinux_2_28_aarch64.whl',
    'd9b9bebb39167442bacf627958d57ae95e452550ced1b68337cbf2a73375856a', 'libraylib_comma.a'),
}

def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--arch', choices=WHEELS, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument("--shared", action="store_true", help="build GNU runtime plugin from the verified native archive")
  parser.add_argument("--cxx", default="g++", help="GNU compiler matching --arch when --shared is used")
  args = parser.parse_args()
  url, digest, archive = WHEELS[args.arch]
  with urllib.request.urlopen(url, timeout=120) as response:
    data = response.read()
  if hashlib.sha256(data).hexdigest() != digest:
    raise ValueError('raylib wheel SHA256 differs from uv.lock')
  expected = [f'lib/{archive}', 'include/raylib.h', 'include/rlgl.h', 'include/raymath.h']
  with zipfile.ZipFile(io.BytesIO(data)) as wheel:
    for relative in expected:
      matches = [name for name in wheel.namelist() if name.endswith('/raylib/install/' + relative)]
      if len(matches) != 1:
        raise ValueError(f'expected exactly one native raylib member: {relative}')
      target = args.output / relative
      target.parent.mkdir(parents=True, exist_ok=True)
      target.write_bytes(wheel.read(matches[0]))
  (args.output / 'SOURCE_SHA256').write_text(digest + '\n')
  if args.shared:
    marker = args.output / 'plugin_contract.cc'
    marker.write_text('extern "C" const char *openpilot_startup_ui_raylib_contract() { return "comma-deps-raylib-6.0.0.1.post103"; }\n')
    libraries = ['EGL', 'GLESv2', 'drm', 'gbm'] if args.arch == 'aarch64' else ['GL', 'X11']
    command = [args.cxx, '-shared', '-fPIC', '-Wl,-Bsymbolic', '-Wl,-z,defs', str(marker), '-Wl,--whole-archive',
               str(args.output / 'lib' / archive), '-Wl,--no-whole-archive', *['-l' + name for name in libraries],
               '-lm', '-ldl', '-lpthread', '-lrt', '-o', str(args.output / 'lib' / 'libopenpilot-raylib.so')]
    subprocess.run(command, check=True)
    (args.output / 'plugin-build.json').write_text(json.dumps({'wheel': url, 'sha256': digest, 'arch': args.arch, 'command': command}, indent=2) + '\n')
  print(f'STARTUP_UI_RAYLIB_ROOT={args.output.resolve()}')

if __name__ == '__main__':
  main()

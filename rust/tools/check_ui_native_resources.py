"""Run the shared CXX ASan surface with a disk preflight before each compiler invocation."""

import argparse
import json
import os
from pathlib import Path
import shutil
from check_startup_ui import Context, adapter_asan


class ResourceContext(Context):
  def run(self, name, command, extra_env=None):
    if 'build' in name:
      free = shutil.disk_usage(self.output).free
      (self.output / f'{name}-disk.json').write_text(json.dumps({'free_bytes': free, 'estimated_growth_bytes': 1024**3}))
      assert free >= 26 * 1024**3, '25 GiB reserve plus 1 GiB build growth required'
    super().run(name, command, extra_env)


parser = argparse.ArgumentParser()
parser.add_argument('--target', type=Path, required=True)
parser.add_argument('--raylib', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
env = dict(os.environ, DISPLAY=args.display, STARTUP_UI_RAYLIB_LIBRARY=str(args.raylib / 'lib/libopenpilot-raylib.so'))
adapter_asan(ResourceContext(root, args.target, args.output, args.raylib, env, 'g++', '1.94.0'))
(args.output / 'result.json').write_text(
  json.dumps({'clear_pixels_exact': True, 'idempotent_dynamic_texture_release': True, 'asan': 'PASS', 'driver_instrumented': False})
)
print('PASS: native CXX ASan, actual background clear pixels, repeated/invalid texture releases and surface destruction')

"""Portable complete shared UI gate; caller supplies a built target and an Xvfb display."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--target', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
checks = [
  ('widgets', 'check_ui_widgets.py', 'widget_trace', False),
  ('scroll', 'check_ui_scroll.py', 'scroll_trace', False),
  ('navigation', 'check_ui_navigation.py', 'navigation_trace', False),
  ('stack', 'check_ui_stack.py', 'stack_trace', False),
  ('scroller', 'check_ui_scroller.py', 'scroller_trace', False),
  ('render', 'check_ui_render.py', 'ui_render', True),
  ('forms', 'check_ui_forms.py', 'form_render', True),
  ('network', 'check_ui_network.py', 'network_render', True),
  ('polygon', 'check_ui_polygon.py', 'polygon_render', True),
  ('diagnostics', 'check_ui_diagnostics.py', 'diagnostic_render', True),
  ('application', 'check_ui_application.py', 'application_run', True),
  ('fps', 'check_ui_fps.py', 'fps_trace', False),
  ('egl', 'check_ui_egl.py', 'egl_trace', False),
]
ledger = []
for name, script, binary, display in checks:
  if name == 'egl':
    available = shutil.disk_usage(args.output).free
    required = 26 * 1024**3
    assert available >= required, 'requires 25GiB floor plus 1GiB fixture build allowance'
    (args.output / 'egl-build-disk.json').write_text(json.dumps({'available_bytes': available, 'required_bytes': required}, indent=2))
  command = [sys.executable, str(root / 'rust/tools' / script), '--binary', str(args.target / 'examples' / binary), '--output', str(args.output / name)]
  if display:
    command += ['--display', args.display]
  with (args.output / f'{name}.log').open('w') as log:
    log.write('Invocation: ' + json.dumps(command) + '\n')
    log.flush()
    result = subprocess.run(command, cwd=root, env=os.environ, stdout=log, stderr=subprocess.STDOUT)
  ledger.append({'scenario': name, 'invocation': command, 'exit_code': result.returncode, 'artifact': str(args.output / f'{name}.log')})
  (args.output / 'invocations.json').write_text(json.dumps(ledger, indent=2))
  print(f'{name}: exit {result.returncode}', flush=True)
  if result.returncode:
    raise SystemExit(result.returncode)
print('PASS: complete shared UI source/native state, render, input, native lifecycle, recording, logging and EGL gates')

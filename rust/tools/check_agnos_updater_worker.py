"""Execute unchanged source worker methods and Rust worker against owned commands."""
import argparse
import ast
from collections import deque
import json
from pathlib import Path
import re
import subprocess
from types import SimpleNamespace

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--launcher', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
results = []
cases = [
  ('success', ['Downloading_SYSTEM: 37', 'verified inactive slot: 100'], 0),
  ('fatal', ['verifying_system: 83', 'fatal image hash mismatch'], 9),
  ('empty-fatal', [], 7),
  ('malformed', ['ignored: -4', 'bad: 1000', 'label: part: 999', 'wrong: 23 suffix'], 3),
  ('unicode', ['ΟΔΌΣ_SYSTEM: ١٣', 'space: \u00a042', 'fullwidth: ９９'], 2),
  ('log-tail', ['ordinary log ' + str(i) for i in range(14)] + ['x' * 300], 1),
  ('signal', ['partial: 3'], -15),
  ('stderr', ['fatal stderr output'], 4),
  ('missing', [], None),
]
for layout in ['tici', 'mici']:
  source_path = root / f'openpilot/system/ui/{layout}_updater.py'
  source = ast.parse(source_path.read_text())
  original = next(node for node in source.body if isinstance(node, ast.ClassDef) and node.name == 'Updater')
  methods = [node for node in original.body if isinstance(node, ast.FunctionDef) and node.name in ['_run_update_process', '_handle_process_line']]
  klass = ast.ClassDef(name='Updater', bases=[], keywords=[], body=methods, decorator_list=[])
  module = ast.fix_missing_locations(ast.Module(body=[klass], type_ignores=[]))
  for name, lines, code in cases:
    directory = (args.output / f'{layout}-{name}').resolve()
    directory.mkdir(exist_ok=True)
    updater = directory / 'updater'
    manifest = directory / 'manifest.json'
    manifest.write_text('[]')
    if code is not None:
      payload = '\n'.join(lines) + ('\n' if lines else '')
      updater.write_text('#!/bin/sh\ncat ' + ('>&2 ' if name == 'stderr' else '') + '<<\'OWNED_PROGRESS\'\n' + payload + 'OWNED_PROGRESS\n' + ('kill -TERM $$\n' if code < 0 else 'exit ' + str(code) + '\n'))
      updater.chmod(0o755)
    else:
      updater.unlink(missing_ok=True)
    rebooted = []
    namespace = {'re': re, 'subprocess': subprocess, 'HARDWARE': SimpleNamespace(reboot=lambda: rebooted.append(True))}
    exec(compile(module, str(source_path), 'exec'), namespace)
    worker = namespace['Updater']()
    worker.updater, worker.manifest = str(updater), str(manifest)
    worker.progress_value = 0
    worker.progress_text = 'Starting update...' if layout == 'tici' else 'starting update'
    worker._last_output = deque(maxlen=12)
    worker.failure_detail = ''
    worker.show_reboot_button = False
    worker._failure_reason = None
    worker._run_update_process()
    expected = {'layout': layout, 'progress': worker.progress_value, 'text': worker.progress_text, 'failure': worker.failure_detail or None if layout == 'tici' else worker._failure_reason, 'running': False, 'reboot_requested': bool(rebooted), 'logs': list(worker._last_output)}
    (directory / 'source.json').write_text(json.dumps(expected, ensure_ascii=True, indent=2))
    command = [str(args.binary.resolve()), str(updater), str(manifest), str(args.launcher.resolve()), layout, str(directory / 'native.json')]
    with (directory / 'native.log').open('w') as log:
      log.write('COMMAND: ' + json.dumps(command) + '\n')
      log.flush()
      process = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=8)
      log.write(f'EXIT: {process.returncode}\n')
    actual = json.loads((directory / 'native.json').read_text())
    result = {'case': layout + '-' + name, 'pass': actual == expected, 'expected': expected, 'actual': actual}
    results.append(result)
    (args.output / 'results.json').write_text(json.dumps(results, ensure_ascii=True, indent=2))
    print(result['case'], 'PASS' if result['pass'] else 'FAIL', flush=True)
assert all(item['pass'] for item in results)

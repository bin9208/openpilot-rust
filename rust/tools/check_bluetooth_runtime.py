import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--bin-dir', required=True, type=Path)
  parser.add_argument('--msgq-python', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  binary, output = args.bin_dir.resolve(), args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  environment = dict(os.environ, PYTHONPATH=f'{args.msgq_python.resolve()}:{ROOT}:{ROOT / "rust/tools"}')
  shim = output / 'input-fixture.so'
  with (output / 'fixture-build.log').open('w') as log:
    subprocess.run(['cc', '-shared', '-fPIC', '-O2', '-Wall', '-Wextra', '-Werror',
                    str(ROOT / 'rust/tools/bluetooth_input_fixture.c'), '-o', str(shim), '-ldl'],
                   check=True, stdout=log, stderr=subprocess.STDOUT)
  cases = [
    ('gestures', 'gesture_fixture', []),
    ('config', 'config_fixture', []),
    ('journal', 'journal_fixture', []),
    ('files', 'file_fixture', []),
    ('engine', 'engine_fixture', []),
    ('input', 'input_fixture', ['--shim', str(shim)]),
    ('input_decode', 'input_fixture', []),
    ('enumerate', 'input_fixture', []),
    ('daemon', '../openpilot-bluetoothd', ['--shim', str(shim)]),
    ('shutdown', '../openpilot-bluetoothd', ['--shim', str(shim)]),
    ('bluez_policy', 'bluez_policy', []),
    ('bluez', 'bluez_client', []),
    ('bluez_prompts', 'bluez_client', ['--deadlines']),
    ('bluez_invalid', 'bluez_client', []),
  ]
  receipts = []
  for name, example, extra in cases:
    command = [sys.executable, str(ROOT / f'rust/tools/check_bluetooth_{name}.py'),
               '--binary', str((binary / 'examples' / example).resolve()), '--output', str(output / name), *extra]
    with (output / f'{name}.log').open('w') as log:
      process = subprocess.run(command, env=environment, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=240)
    receipts.append({'case': name, 'command': command, 'returncode': process.returncode})
    (output / 'suite.json').write_text(json.dumps(receipts, indent=2))
    assert process.returncode == 0, receipts[-1]
    print(f'PASS {name}', flush=True)


if __name__ == '__main__':
  main()

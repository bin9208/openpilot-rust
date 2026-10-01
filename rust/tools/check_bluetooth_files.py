# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from openpilot.selfdrive.carrot.bluetooth.model import atomic_json


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  assert shutil.disk_usage(ROOT).free >= 26 * 1024**3
  args.output.mkdir(parents=True, exist_ok=True)
  values = [None, True, False, 0, -0.0, 1e-7, 1e20, float('inf'), float('-inf'), float('nan'),
            {'z': '한😀\u0000\u007f', 'a': [True, None, 10**200]}, '\ud800', '\udfff', {'\ud800': 'value'},
            ''.join(chr(point) for point in range(0x110000) if not 0xd800 <= point <= 0xdfff)]
  payload = ''.join(json.dumps(value) + '\n' for value in values)
  (args.output / 'input.jsonl').write_text(payload)
  source = args.output / 'original'
  source.mkdir()
  expected = []
  for index, line in enumerate(payload.splitlines()):
    path = source / f'{index}.json'
    path.write_bytes(b'initial')
    path.chmod(0o644)
    try:
      atomic_json(path, json.loads(line))
      expected.append(True)
    except UnicodeEncodeError:
      expected.append(False)
  native_root = args.output.resolve() / 'native'
  process = subprocess.run([str(args.binary.resolve()), str(native_root)], input=payload, text=True,
                           capture_output=True, timeout=30)
  (args.output / 'native.stdout').write_text(process.stdout)
  (args.output / 'native.stderr').write_text(process.stderr)
  process.check_returncode()
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  assert expected == actual, (expected, actual)
  sizes = []
  for index, succeeded in enumerate(expected):
    original, native = source / f'{index}.json', native_root / f'{index}.json'
    left, right = original.read_bytes(), native.read_bytes()
    assert left == right, index
    mode = 0o600 if succeeded else 0o644
    assert original.stat().st_mode & 0o777 == mode
    assert native.stat().st_mode & 0o777 == mode
    sizes.append(len(right))
  assert len(list(source.iterdir())) == len(values)
  assert len(list(native_root.iterdir())) == len(values)
  summary = {'cases': len(values), 'all_unicode_scalar_values': 0x110000 - 0x800,
             'unicode_errors': expected.count(False), 'bytes': sum(sizes), 'comparison': 'exact bytes and permissions',
             'source_sha256': hashlib.sha256((ROOT / 'openpilot/selfdrive/carrot/bluetooth/model.py').read_bytes()).hexdigest(),
             'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(summary, indent=2))
  print(json.dumps(summary))


if __name__ == '__main__':
  main()

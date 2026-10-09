from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--library', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  library = args.library.resolve()
  link = args.evidence / 'libusb-1.0.so.0'
  if not link.exists():
    link.symlink_to(library)
  rows = []
  for mode in ['ok', 'control_error', 'control_short', 'bulk_ok', 'partial', 'bulk_short']:
    lifetime = (args.evidence / f'{mode}-lifetime.log').resolve()
    lifetime.write_text('')
    command = [str(args.binary)]
    process = subprocess.run(
      command,
      capture_output=True,
      text=True,
      timeout=10,
      env={**os.environ, 'LD_LIBRARY_PATH': str(args.evidence.resolve()), 'USB_FIXTURE_MODE': mode, 'USB_FIXTURE_LOG': str(lifetime)},
    )
    (args.evidence / f'{mode}.log').write_text(process.stdout + process.stderr)
    lines = lifetime.read_text().splitlines()
    count = sum(line in {'control', 'bulk'} for line in lines)
    expected = 2 if mode in {'ok', 'bulk_ok'} else 1
    passed = process.returncode == 0 and count == expected and lines[-1:] == ['exit clean']
    rows.append(
      {
        'mode': mode,
        'invocation': command,
        'exit_code': process.returncode,
        'dependent_calls': count,
        'expected_calls': expected,
        'resources_released': lines[-1:] == ['exit clean'],
        'passed': passed,
        'lifetime_artifact': str(lifetime),
      }
    )
    print(mode, 'PASS' if passed else 'FAIL', flush=True)
  manifest = {
    'results': rows,
    'binary_sha256': hashlib.file_digest(args.binary.open('rb'), 'sha256').hexdigest(),
    'library_sha256': hashlib.file_digest(library.open('rb'), 'sha256').hexdigest(),
  }
  (args.evidence / 'comparison.json').write_text(json.dumps(manifest, indent=2) + '\n')
  assert all(row['passed'] for row in rows)


if __name__ == '__main__':
  main()

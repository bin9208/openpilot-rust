import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import struct
import subprocess
import sys

WIRE = struct.Struct('@llHHi')


def line(process: subprocess.Popen[str]) -> str:
  assert process.stdout is not None
  ready, _, _ = select.select([process.stdout], [], [], 8)
  assert ready, 'input fixture output timed out'
  result = process.stdout.readline()
  assert result, 'input fixture closed output'
  return result


def run_case(command: list[str], directory: Path, shim: Path, flags: dict[str, str]) -> dict:
  directory.mkdir(parents=True)
  fifo = directory / 'input-fifo'
  os.mkfifo(fifo, 0o600)
  keeper = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
  fake = directory / 'commands'
  fake.mkdir()
  script = Path(__file__).with_name('bluetooth_sudo_fixture.py')
  sudo = fake / 'sudo'
  if flags.get('INPUT_FIXTURE_SUDO_MODE') != 'missing':
    sudo.write_bytes(script.read_bytes())
    sudo.chmod(0o700)
  logical = '/dev/input/event99999' if flags.get('INPUT_FIXTURE_DENIED') else str(directory / 'logical-input')
  if flags.get('INPUT_FIXTURE_BAD_PARENT'):
    logical = str(directory / 'event99999')
  environment = os.environ | flags | {
    'LD_PRELOAD': str(shim.resolve()), 'INPUT_FIXTURE_PATH': logical,
    'INPUT_FIXTURE_FIFO': str(fifo.resolve()), 'INPUT_FIXTURE_LOG': str((directory / 'calls.log').resolve()),
    'INPUT_FIXTURE_SUDO_LOG': str((directory / 'sudo.jsonl').resolve()),
    'PATH': str(fake.resolve()),
  }
  stale = WIRE.pack(1, 1, 1, 115, 1) * 96
  assert os.write(keeper, stale) == len(stale)
  observed = []
  with (directory / 'stderr.log').open('w') as error:
    process = subprocess.Popen(command + [logical], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error, text=True)
    try:
      first = json.loads(line(process))
      if 'fd' in first:
        first.pop('fd')
      observed.append(first)
      if first.get('opened'):
        assert process.stdin is not None

        def read() -> None:
          process.stdin.write('"read"\n')
          process.stdin.flush()
          observed.append(json.loads(line(process)))

        read()
        payload = b''.join(WIRE.pack(123 + i, 456789, i % 2, i, -i) for i in range(130))
        assert os.write(keeper, payload) == len(payload)
        read()
        read()
        read()
        assert os.write(keeper, b'x' * 13) == 13
        read()
        os.close(keeper)
        keeper = -1
        read()
        process.stdin.close()
        observed.append(json.loads(line(process)))
      process.wait(timeout=5)
      assert process.returncode == 0
    finally:
      if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
      if keeper >= 0:
        os.close(keeper)
  for item in observed:
    if 'error' in item:
      item['error'] = item['error'].replace(str(directory), '<fixture>')
  result = {'observed': observed, 'calls': (directory / 'calls.log').read_text().splitlines(),
            'sudo': [json.loads(value) for value in (directory / 'sudo.jsonl').read_text().splitlines()] if (directory / 'sudo.jsonl').exists() else []}
  (directory / 'result.json').write_text(json.dumps(result, indent=2))
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--shim', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  source = [sys.executable, str(Path(__file__).with_name('bluetooth_input_source.py'))]
  cases = [('normal', {}), ('grab-error', {'INPUT_FIXTURE_FAIL_GRAB': '1'}),
           ('clock-error', {'INPUT_FIXTURE_FAIL_CLOCK': '1'})]
  cases.extend((f'permission-{mode}', {'INPUT_FIXTURE_DENIED': '1', 'INPUT_FIXTURE_SUDO_MODE': mode})
               for mode in ('ok', 'fail-chgrp', 'fail-chmod', 'timeout', 'loud', 'signal', 'missing'))
  for stage in ('drain', 'active'):
    for interrupted in (False, True):
      flags = {'INPUT_FIXTURE_READ_STAGE': stage}
      if interrupted:
        flags['INPUT_FIXTURE_READ_INTR'] = '1'
      cases.append((f'read-{stage}-{interrupted}', flags))
  cases.extend([('not-char', {'INPUT_FIXTURE_DENIED': '1', 'INPUT_FIXTURE_NOT_CHAR': '1'}),
                ('bad-parent', {'INPUT_FIXTURE_DENIED': '1', 'INPUT_FIXTURE_BAD_PARENT': '1'})])
  differences = []
  for name, flags in cases:
    expected = run_case(source, args.output / name / 'source', args.shim, flags)
    actual = run_case([str(args.binary.resolve()), 'open'], args.output / name / 'native', args.shim, flags)
    if expected != actual:
      differences.append(name)
  result = {'cases': len(cases), 'differences': differences, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))
  assert not differences, differences


if __name__ == '__main__':
  main()

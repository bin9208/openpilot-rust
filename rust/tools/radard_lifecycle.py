# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "pyzmq", "pyserial", "requests", "setproctitle", "zstandard", "numpy"]
# ///
from __future__ import annotations

import json
from pathlib import Path
import resource
import signal
import subprocess
import tempfile
import time
from typing import Final

from openpilot.cereal import car
from radard_ipc import ROOT, Mode, Paths, arguments, command, environment

EXPECTED_EXITS: Final = {
  'waiting-int': {'source': -signal.SIGINT, 'rust': -signal.SIGINT},
  'waiting-term': {'source': -signal.SIGINT, 'rust': -signal.SIGINT},
  'malformed-carparams': {'source': 1, 'rust': 1},
  'bad-settings': {'source': -signal.SIGABRT, 'rust': 1},
}


def run(mode: Mode, paths: Paths, case: str) -> int:
  folder = paths.output / f'{mode}-{case}'
  folder.mkdir()
  with tempfile.TemporaryDirectory(prefix='msgq_radard_lifecycle_', dir='/dev/shm') as shared:
    prefix = Path(shared).name.removeprefix('msgq_')
    params = folder / 'params' / prefix
    params.mkdir(parents=True)
    (params / 'EnableRadarTracks').write_text('invalid' if case == 'bad-settings' else '1')
    (params / 'EnableCornerRadar').write_text('0')
    if case == 'malformed-carparams':
      (params / 'CarParams').write_bytes(b'not capnp')
    if case == 'bad-settings':
      (params / 'CarParams').write_bytes(car.CarParams.new_message(brand='hyundai').to_bytes())
    with (folder / 'stderr.log').open('wb') as stderr, (folder / 'stdout.log').open('wb') as stdout:
      child = subprocess.Popen(command(mode, paths), env=environment(params, prefix), cwd=ROOT, stdout=stdout, stderr=stderr)
      try:
        deadline = time.monotonic() + 8.0
        while child.poll() is None and time.monotonic() < deadline:
          if 'is waiting for CarParams' in (folder / 'stderr.log').read_text():
            break
          time.sleep(0.01)
        if case.startswith('waiting'):
          assert child.poll() is None
          time.sleep(0.12)
          child.send_signal(signal.SIGINT if case.endswith('int') else signal.SIGTERM)
        status = child.wait(timeout=3)
      finally:
        if child.poll() is None:
          child.kill()
          child.wait(timeout=3)
  return status


def main() -> None:
  paths, selected = arguments(tuple(EXPECTED_EXITS))
  paths.output.mkdir(parents=True)
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  rows = []
  for mode in ('source', 'rust'):
    for case in ([selected] if selected is not None else EXPECTED_EXITS):
      status = run(mode, paths, case)
      expected = EXPECTED_EXITS[case][mode]
      row = {'mode': mode, 'scenario': case, 'exit': status, 'expected_exit': expected,
             'command': command(mode, paths), 'artifact': str(paths.output / f'{mode}-{case}')}
      rows.append(row)
      (paths.output / 'receipt.json').write_text(json.dumps({'status': 'RUNNING', 'cases': rows}, indent=2) + '\n')
      print(json.dumps(row), flush=True)
      assert status == expected, row
  receipt = {'status': 'PASS', 'cases': rows, 'numeric_error_boundary': 'original C++ Params SIGABRT; typed Rust fatal startup exit 1'}
  (paths.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print('PASS source/native lifecycle, including declared numeric-error boundary', flush=True)


if __name__ == '__main__':
  main()

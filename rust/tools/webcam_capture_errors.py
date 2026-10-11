"""Observe translated external getter failures and worker teardown on real I/O error."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('output', type=Path)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--workers', type=Path, required=True)
  parser.add_argument('--fault-shim', type=Path, required=True)
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--reserve-mib', type=int, default=32)
  args = parser.parse_args()
  output = args.output.resolve()
  assert shutil.disk_usage(output.parent).free >= 25 * 1024**3 + args.reserve_mib * 1024**2
  output.mkdir(exist_ok=False)
  rows = []
  for name in ('get', 'closed', 'observer'):
    folder = output / name
    folder.mkdir()
    shutil.copyfile(args.source / 'road.mkv', folder / 'road.mkv')
    environment = dict(os.environ, WEBCAM_OWNED_ROOT=str(folder))
    if name == 'observer':
      (folder / 'roadCameraState-0.bin').mkdir()
      specification = folder / 'spec.json'
      specification.write_text(json.dumps([{'kind': 'road', 'input': str(folder / 'road.mkv')}]))
      argv = [str(args.workers), str(folder), str(specification)]
      input_ = 'RUN\n'
    else:
      environment['LD_PRELOAD'] = str(args.fault_shim)
      environment['WEBCAM_CAPTURE_FAULT'] = name
      argv = [str(args.trace), 'capture', str(folder / 'road.mkv'), str(folder / 'frames')]
      input_ = None
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix='msgq_webcam255_', dir='/dev/shm') as namespace:
      environment['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      result = subprocess.run(argv, env=environment, input=input_, text=True, capture_output=True, timeout=3)
      removed = not Path(f'/tmp/{environment["OPENPILOT_PREFIX"]}_visionipc_camerad').exists()
    row = {
      'case': name,
      'argv': argv,
      'returncode': result.returncode,
      'stdout': result.stdout,
      'stderr': result.stderr,
      'elapsed': time.monotonic() - started,
      'listener_removed': removed,
      'elf_sha256': hashlib.sha256(Path(argv[0]).read_bytes()).hexdigest(),
      'shim_sha256': hashlib.sha256(args.fault_shim.read_bytes()).hexdigest(),
    }
    (folder / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
    assert result.returncode == 1 and removed, row
    expected = {'get': 'owned capture get failure', 'closed': 'owned capture closed-state failure', 'observer': 'IsADirectory'}[name]
    assert expected in result.stderr, row
    if name == 'closed':
      assert len(list((folder / 'frames').glob('*.nv12'))) == 8
      for index in range(8):
        assert (folder / f'frames/{index}.nv12').read_bytes() == (args.source / f'vision-0-{index}.nv12').read_bytes()
    rows.append(row)
  (output / 'result.json').write_text(json.dumps(rows, indent=2) + '\n')
  print(json.dumps(rows, indent=2))


if __name__ == '__main__':
  main()

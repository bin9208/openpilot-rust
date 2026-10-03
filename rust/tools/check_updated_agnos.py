#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from agnos_fixture import Fixture, Server
from check_agnos import run_trace


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  server = Server()
  try:
    with tempfile.TemporaryDirectory(prefix='updated-agnos-') as directory:
      root = Path(directory)
      for scenario in ('download', 'casync', 'corruption'):
        snapshots = []
        for implementation in ('source', 'native'):
          folder = root / f'{scenario}-{implementation}'
          folder.mkdir()
          fixture = Fixture(folder, server, args.target / 'debug/openpilot-process-child', scenario)
          server.mode = ''
          fixture.request['operations'] = [
            {'op': 'target'}, {'op': 'flash', 'standalone': False, 'retry_network': False},
          ]
          if implementation == 'source':
            trace, stderr = run_trace([sys.executable, str(Path(__file__).with_name('agnos_source.py'))], fixture, server)
            succeeded = 'result' in trace['rows'][-1]
            stdout = json.dumps(trace)
          else:
            config = folder / 'config.json'
            config.write_text(json.dumps(fixture.config))
            server.reset()
            result = subprocess.run([
              str(args.target / 'debug/examples/updated_agnos'), str(config), str(fixture.manifest),
              f'ipc://{folder}/logs',
            ], capture_output=True, text=True, timeout=20)
            succeeded = result.returncode == 0
            stdout, stderr = result.stdout, result.stderr
            if succeeded:
              assert json.loads(stdout.splitlines()[-1]) == {'target_slot': 1}
          snapshot = fixture.snapshot() | {'requests': list(server.requests), 'succeeded': succeeded}
          assert snapshot['calls'] == [['--boot_slot'], ['--set_unbootable', '1']], snapshot
          assert succeeded == (scenario != 'corruption'), (scenario, stdout, stderr)
          (args.output / f'{scenario}-{implementation}.stdout').write_text(stdout)
          (args.output / f'{scenario}-{implementation}.stderr').write_text(stderr)
          snapshots.append(snapshot)
        assert snapshots[0] == snapshots[1], (scenario, snapshots)
        (args.output / f'{scenario}.json').write_text(json.dumps(snapshots[1], indent=2) + '\n')
      print('PASS: background AGNOS adapter source-equivalent compressed/casync writes, slot selection and corrupt-image failure')
  finally:
    server.close()


if __name__ == '__main__':
  main()

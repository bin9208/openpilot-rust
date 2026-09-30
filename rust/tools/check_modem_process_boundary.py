#!/usr/bin/env python3
"""Owned descriptor/exec and source startup-order regression; no modem or network access."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from modem_fixture import Fixture
from modem_source import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  results = {}
  with tempfile.TemporaryDirectory(prefix='modem-process-') as directory:
    root = Path(directory)
    fixture = Fixture(root)
    fixture.config['launcher'] = str(args.target / 'debug/openpilot-process-child')
    try:
      marker = root / 'inherited'
      with marker.open('w') as inherited:
        os.set_inheritable(inherited.fileno(), True)
        helper = root / 'helper'
        helper.write_text(f'''#!{sys.executable}
import json, os, sys
from pathlib import Path
root = Path({str(root)!r})
leaked = any(os.path.realpath('/proc/self/fd/' + fd) == {str(marker)!r} for fd in os.listdir('/proc/self/fd'))
with (root / 'descriptor.jsonl').open('a') as f:
  f.write(json.dumps({{'argv': sys.argv[1:], 'leaked': leaked}}) + '\\n')
if sys.argv[1:4] == ['ip', 'rule', 'del']: sys.exit(1)
if sys.argv[1:2] == ['-4']: print('inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0')
''')
        helper.chmod(0o755)
        fixture.config.update(sudo=str(helper), ip=str(helper))
        request = {
          'config': fixture.config,
          'operations': [
            {'op': 'step', 'state': 'CONNECTING'},
            {'op': 'wait_exit'},
            {'op': 'kill'},
            {'op': 'poll'},
            {'op': 'routes', 'ip': '10.0.0.2', 'peer': '10.0.0.1'},
          ],
        }
        child = subprocess.run(
          [args.target / 'debug/examples/modem_trace'], input=json.dumps(request), text=True, capture_output=True, pass_fds=(inherited.fileno(),), timeout=10
        )
        assert child.returncode == 0, child.stderr
        rows = [json.loads(line) for line in (root / 'descriptor.jsonl').read_text().splitlines()]
        assert len(rows) >= 8 and all(not row['leaked'] for row in rows), rows
        assert any(row['argv'][0] == 'pppd' for row in rows)
        assert any(row['argv'][0] == '-4' for row in rows)
        results['descriptor_closure'] = rows
      bad = root / 'no-shebang'
      escaped = root / 'shell-fallback'
      bad.write_text(f'touch {escaped}\n')
      bad.chmod(0o755)
      fixture.config['sudo'] = str(bad)
      failed = subprocess.run(
        [args.target / 'debug/examples/modem_trace'],
        input=json.dumps({'config': fixture.config, 'operations': [{'op': 'kill'}]}),
        text=True,
        capture_output=True,
        timeout=5,
      )
      assert failed.returncode != 0 and not escaped.exists(), failed
      results['exec_failure'] = {'exit': failed.returncode, 'stderr': failed.stderr, 'shell_fallback_marker': escaped.exists()}
      fixture.config['sudo'] = str(helper)
      fixture.config['state'] = str(root / 'missing-parent/state')
      (root / 'descriptor.jsonl').unlink()
      config = root / 'config.json'
      config.write_text(json.dumps(fixture.config))
      failed = subprocess.run([args.target / 'debug/openpilot-modem', '--config', config], capture_output=True, text=True, timeout=5)
      assert failed.returncode != 0 and not (root / 'descriptor.jsonl').exists(), failed
      module, modem = load(fixture.config)
      source_error = None
      try:
        modem.run()
        modem.stop()
      except OSError as error:
        source_error = str(error)
      assert source_error and not (root / 'descriptor.jsonl').exists()
      results['startup_order'] = {'native_exit': failed.returncode, 'native_stderr': failed.stderr, 'source_error': source_error, 'external_commands': []}
    finally:
      fixture.close()
  (args.evidence / 'process-boundary.json').write_text(json.dumps(results, indent=2) + '\n')
  print('PASS descriptor closure, exec failure without shell fallback, and source startup-error cleanup order')


if __name__ == '__main__':
  main()

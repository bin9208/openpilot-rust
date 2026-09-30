"""Collect manager lifecycle/default diagnostics with a minimal synthetic environment."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

import zmq


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('binding', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.binary, args.binding, args.output = (path.resolve() for path in (args.binary, args.binding, args.output))
  args.output.mkdir(parents=True, exist_ok=True)
  # No host or CI environment is inherited into the log records.
  with tempfile.TemporaryDirectory(prefix='manager-log-') as socket_dir:
    prefix = Path(socket_dir).name
    endpoint = f'ipc:///tmp/logmessage{prefix}'
    env = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C.UTF-8', 'LC_CTYPE': 'C.UTF-8', 'PYTHONPATH': str(Path.cwd()),
           'OPENPILOT_PREFIX': prefix, 'MANAGER_LOG_CAPTURE': endpoint, 'NOBOARD': '', 'BLOCK': 'ui,,custom',
           'FIXTURE_LABEL': 'synthetic "quotes" and \'apostrophe\'\nline'}
    if 'LD_LIBRARY_PATH' in os.environ:
      env['LD_LIBRARY_PATH'] = os.environ['LD_LIBRARY_PATH']
    (args.output / 'synthetic-environment.json').write_text(json.dumps(env, indent=2))
    captured = {}
    for kind in ('source', 'native'):
      output = args.output / kind
      with zmq.Context() as context, context.socket(zmq.PULL) as collector:
        collector.bind(endpoint)
        command = ([sys.executable, 'rust/tools/manager_reference.py', str(args.binding)] if kind == 'source' else [str(args.binary)])
        with (args.output / f'{kind}.log').open('w') as log:
          process = subprocess.Popen([*command, str(output), 'logging'], env=env, stdout=log, stderr=subprocess.STDOUT)
          records = []
          deadline = time.monotonic() + 60
          try:
            while process.poll() is None or collector.poll(100):
              if collector.poll(100):
                packet = collector.recv()
                records.append(json.loads(packet[1:]))
              if time.monotonic() > deadline:
                raise TimeoutError(kind)
            assert process.wait() == 0, kind
          finally:
            if process.poll() is None:
              process.kill()
              process.wait()
      (args.output / f'{kind}-records.json').write_text(json.dumps(records, indent=2))
      selected = [{'msg': item['msg'], 'daemon': item['ctx'].get('daemon'), 'level': item['level']} for item in records]
      captured[kind] = selected
    assert captured['source'] == captured['native'], (captured['source'], captured['native'])
    messages = captured['native']
    assert len(messages) == 4, messages
    assert messages[0] == {'msg': "Failed to cast param b'UptimeOnroad' with value=b'1__2.5' from type t=<ParamKeyType.FLOAT: 3>",
                           'daemon': None, 'level': 'WARNING'}
    assert messages[1] == {'msg': 'manager start', 'daemon': 'manager', 'level': 'INFO'}
    assert isinstance(messages[2]['msg'], dict) and 'environ' in messages[2]['msg']
    assert messages[2]['daemon'] == 'manager' and messages[2]['level'] == 'INFO'
    assert messages[3] == {'msg': 'everything is dead', 'daemon': 'manager', 'level': 'INFO'}
    source = json.loads((args.output / 'source/result.json').read_text())
    native = json.loads((args.output / 'native/result.json').read_text())
    assert source == native, 'lifecycle hooks changed source order or Params'
    trace = native['trace']
    assert trace.index(['lifecycle', 'start']) < trace.index(['put', 'RecordAudio', '0'])
    assert trace.index(['stop', False]) < trace.index(['stop', True]) < trace.index(['lifecycle', 'cleanup_finished']) < trace.index(['exit', 'uninstall'])
    (args.output / 'summary.json').write_text(json.dumps({'records': messages, 'source_order': True, 'synthetic_environment_only': True}, indent=2))
    print('PASS manager lifecycle/default collector: 4 matching records, daemon context, full synthetic environment and exact transition/cleanup order')


if __name__ == '__main__':
  main()

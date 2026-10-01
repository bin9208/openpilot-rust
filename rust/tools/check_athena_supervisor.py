#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time
from athena_fixture import daemon, private_environment, websocket_server, wait_for


def children(pid):
  return [int(value) for value in Path(f'/proc/{pid}/task/{pid}/children').read_text().split()]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('capture', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False}
  with websocket_server() as (port, connected, _), private_environment(port) as env:
    capture_log = (args.output / 'records.jsonl').open('w')
    capture = subprocess.Popen([args.capture], env=env.env, stdout=capture_log, stderr=subprocess.STDOUT)
    try:
      wait_for(lambda: 'READY' in (args.output / 'records.jsonl').read_text())
      (env.params / 'AthenadPid').write_text('12345')
      with daemon(args.binary, args.output, env.env) as (process, pid):
        first = connected.get(timeout=10)
        assert first.rpc('echo', ['first'])['result'] == 'first'
        child = wait_for(lambda: children(pid))[0]
        assert Path(f'/proc/{child}/exe').resolve().name == 'openpilot-athenad'
        metadata = json.loads((env.root / 'build.json').read_text())
        metadata['openpilot']['version'] = '9.9.9'
        (env.root / 'build.json').write_text(json.dumps(metadata))
        killed = time.monotonic()
        os.kill(child, signal.SIGKILL)
        second = connected.get(timeout=9)
        delay = time.monotonic() - killed
        assert 5 <= delay < 8, delay
        replacement = wait_for(lambda: children(pid))[0]
        assert replacement != child and not Path(f'/proc/{child}').exists()
        assert second.rpc('echo', ['second'])['result'] == 'second'
        time.sleep(0.2)
        records = [json.loads(line) for line in (args.output / 'records.jsonl').read_text().splitlines()[1:]]
        result['captured_records'] = records
        assert records
        call_records = [record for record in records if isinstance(record.get('msg'), dict) and record['msg'].get('event') == 'athena.jsonrpc_handler.call_method']
        assert len(call_records) >= 2, records
        assert all(record['ctx']['version'] == '1.2.3' for record in call_records), call_records
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=10) == 0
        assert not Path(f'/proc/{replacement}').exists()
        assert not (env.params / 'AthenadPid').exists()
        result.update(first_pid=child, second_pid=replacement, restart_seconds=delay, returncode=process.returncode, child_reaped=True, pid_param_removed=True, **{'pass': True})
    finally:
      capture.send_signal(signal.SIGTERM)
      capture.wait(timeout=5)
      capture_log.close()
      (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: actual native supervisor SIGKILL restart after 5 seconds,inherited logging context across metadata change,SIGTERM child reaping and AthenadPid cleanup')


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import signal
import threading
import time
from athena_fixture import daemon, private_environment, wait_for, websocket_server


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  failed = threading.Event()
  attempts = []

  def handshake(connection, _):
    attempts.append(time.monotonic())
    if len(attempts) == 1:
      failed.set()
      return connection.respond(503, 'synthetic transient failure')
    return None

  result = {'pass': False}
  with websocket_server(ping_interval=None, process_request=handshake) as (port, connected, _), private_environment(port) as env:
    (env.params / 'LastAthenaPingTime').write_text('123')
    try:
      with daemon(args.binary, args.output, env.env) as (process, pid):
        assert failed.wait(5)
        wait_for(lambda: not (env.params / 'LastAthenaPingTime').exists(), 3)
        peer = connected.get(timeout=5)
        assert peer.rpc('echo', ['idle-clock'])['result'] == 'idle-clock'
        started = time.monotonic()
        second = connected.get(timeout=100)
        elapsed = time.monotonic() - started
        assert 89 <= elapsed <= 99, elapsed
        assert second.rpc('echo', ['reconnected-after-silence'])['result'] == 'reconnected-after-silence'
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        result.update(stale_ping_removed=True, silence_seconds=elapsed, handshake_attempts=len(attempts), returncode=process.returncode, pass_=True)
        result['pass'] = True
    finally:
      (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: failed handshake clears stale ping Params; real30-second read timeouts reconnect after >70-second ping age; SIGTERM exits0')


if __name__ == '__main__':
  main()

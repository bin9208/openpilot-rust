from __future__ import annotations

import argparse
import json
from pathlib import Path
import time

from carrot_server_heartbeat_driver import Composition, Driver, until
from carrot_server_heartbeat_peer import Peer, Response, response


def run(binary: Path, output: Path) -> None:
  output.mkdir(parents=True)
  results = []
  for mode in ['silent', 'trickle']:
    for side in ['source', 'native']:
      wire = response(b'a', length=9).wire
      reply = Response(response().wire, hold=True) if mode == 'silent' else Response(wire, parts=(b'b',) * 8, interval=1.0)
      peer = Peer((reply,))
      driver = Driver(side, binary, output / mode / side, peer, params=(('Version', b'owned-version'), ('GithubUsername', b'owned-owner')), composition=Composition('runtime'))
      row = {'mode': mode, 'side': side}
      try:
        until(peer.received.is_set)
        before = driver.command('status')
        began = time.monotonic()
        cleanup = driver.command('server_cleanup')
        row.update(serve_return_seconds=time.monotonic() - began, serve_result=cleanup, worker_incomplete_after_serve=not peer.completed)
        assert row['serve_return_seconds'] < 0.5 and row['worker_incomplete_after_serve']
        assert driver.command('status') == before
        driver.close()
        elapsed = time.monotonic() - began
        row.update(process_exit_seconds=elapsed, exit=driver.process.returncode, requests=peer.rows, completed=peer.completed)
        assert driver.process.returncode == 0
        if mode == 'silent':
          assert 3.0 < elapsed < 4.5
        else:
          assert 7.5 < elapsed < 9.0 and peer.completed == [0]
        results.append(row)
        (output / 'observations.json').write_text(json.dumps(results, indent=2) + '\n')
      finally:
        peer.close()
        if driver.process.poll() is None:
          driver.close()
  (output / 'result.json').write_text(json.dumps({'silent_socket_timeout_pair': True, 'progressing_body_outlives_socket_interval_pair': True, 'source_equivalent_thread_wait_at_process_exit': True, 'global_shutdown_bound_proved': False}, indent=2) + '\n')
  print('PASS')


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  for name in ['binary', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  run(args.binary, args.output)

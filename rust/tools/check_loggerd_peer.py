#!/usr/bin/env python3
"""Verify logger fixture sends wait for processing, using actual native msgq."""

from __future__ import annotations

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from openpilot.cereal import messaging
from loggerd_fixtures import event
from loggerd_peer import Peer


def worker(output: Path) -> None:
  subscriber = messaging.sub_sock('can', timeout=5000)
  (output / 'ready').write_text('ready')
  packet = subscriber.receive()
  assert packet is not None
  received = time.monotonic_ns()
  until = time.monotonic() + 0.1
  while time.monotonic() < until:
    pass
  (output / 'processed.json').write_text(json.dumps({'received': received, 'completed': time.monotonic_ns(), 'sha256': sha256(packet).hexdigest()}))
  while True:
    subscriber.receive()


def check(output: Path) -> dict:
  output = output.resolve()
  output.mkdir(parents=True)
  prefix = f'logger-peer-{os.getpid()}'
  shm = Path('/dev/shm') / ('msgq_' + prefix)
  shm.mkdir()
  previous = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  publisher = messaging.PubMaster(['can'])
  command = [sys.executable, str(Path(__file__).resolve()), '--worker', str(output)]
  with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
    process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
  try:
    deadline = time.monotonic() + 10
    while not (output / 'ready').exists():
      assert process.poll() is None, (output / 'stderr.log').read_text()
      assert time.monotonic() < deadline
      time.sleep(0.001)
    peer = Peer.__new__(Peer)
    peer.root, peer.process, peer.publisher, peer.inputs = output, process, publisher, []
    packet = event('can', 123)
    started = time.monotonic_ns()
    peer.send('can', packet)
    returned = time.monotonic_ns()
    while not (output / 'processed.json').exists():
      assert process.poll() is None
      assert time.monotonic() < deadline
      time.sleep(0.001)
    processed = json.loads((output / 'processed.json').read_text())
    result = {
      'argv': command,
      'started': started,
      'send_returned': returned,
      **processed,
      'processing_finished_before_send_return': processed['completed'] <= returned,
      'packet_exact': processed['sha256'] == sha256(packet).hexdigest(),
    }
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    assert result['packet_exact']
    assert result['processing_finished_before_send_return'], result
    return result
  finally:
    process.terminate()
    process.wait(timeout=5)
    publisher = None
    shutil.rmtree(shm)
    if previous is None:
      os.environ.pop('OPENPILOT_PREFIX', None)
    else:
      os.environ['OPENPILOT_PREFIX'] = previous


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--worker', type=Path)
  parser.add_argument('--output', type=Path)
  args = parser.parse_args()
  if args.worker:
    worker(args.worker)
  else:
    print(json.dumps(check(args.output)))


if __name__ == '__main__':
  main()

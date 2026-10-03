#!/usr/bin/env python3
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import threading

from openpilot.cereal import log, messaging
from torque_ipc import TOPICS, wait_for_input_readers


def emit(value) -> None:
  print(json.dumps(value), flush=True)


def consume() -> None:
  sockets = {name: messaging.sub_sock(name, conflate=True, timeout=2000) for name in TOPICS}
  emit('ready')
  for index in (1, 2):
    pose = sockets['livePose'].receive()
    assert pose is not None
    if index == 1:
      emit('poll-consumed')
      assert sys.stdin.readline().strip() == 'release'
    packets = {'livePose': pose}
    for name, socket in sockets.items():
      if name != 'livePose':
        packet = socket.receive(non_blocking=True)
        if packet is not None:
          packets[name] = packet
    row = {}
    for name, packet in packets.items():
      with log.Event.from_bytes(packet) as event:
        row[name] = event.logMonoTime
    emit(row)


def receive(process):
  assert select.select([process.stdout], [], [], 5)[0], 'fixture did not respond'
  line = process.stdout.readline()
  assert line, process.poll()
  return json.loads(line)


def send(publishers, index: int) -> None:
  for name in sorted(TOPICS, key=lambda value: value == 'livePose'):
    event = messaging.new_message(name, valid=True)
    event.logMonoTime = index
    publishers[name].send(event.to_bytes())


class ObservedPublisher:
  def __init__(self, publisher, entered: threading.Event):
    self.publisher = publisher
    self.entered = entered

  def wait_for_readers(self, timeout: float) -> None:
    self.entered.set()
    self.publisher.wait_for_readers(timeout=timeout)


def scenario(output: Path, fixed: bool) -> list[dict]:
  with tempfile.TemporaryDirectory(prefix='msgq_torque-order-', dir='/dev/shm') as directory:
    prefix = Path(directory).name.removeprefix('msgq_')
    previous = os.environ.get('OPENPILOT_PREFIX')
    os.environ['OPENPILOT_PREFIX'] = prefix
    publishers = {}
    process = None
    try:
      publishers = {name: messaging.pub_sock(name) for name in TOPICS}
      with (output / ('fixed.log' if fixed else 'legacy.log')).open('w') as stderr:
        process = subprocess.Popen([sys.executable, __file__, '--consume'], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=stderr, text=True)
        assert receive(process) == 'ready'
        for publisher in publishers.values():
          publisher.wait_for_readers(timeout=2)
        send(publishers, 1)
        assert receive(process) == 'poll-consumed'
        assert publishers['livePose'].all_readers_updated()
        assert not publishers['carOutput'].all_readers_updated()
        if fixed:
          entered = threading.Event()
          observed = {name: ObservedPublisher(publisher, entered) for name, publisher in publishers.items()}
          with ThreadPoolExecutor(max_workers=1) as executor:
            waiting = executor.submit(wait_for_input_readers, observed)
            assert entered.wait(timeout=2)
            assert not waiting.done(), 'input barrier completed while non-poll input was unread'
            process.stdin.write('release\n')
            process.stdin.flush()
            waiting.result(timeout=3)
          first = receive(process)
          send(publishers, 2)
        else:
          publishers['livePose'].wait_for_readers(timeout=2)
          send(publishers, 2)
          process.stdin.write('release\n')
          process.stdin.flush()
          first = receive(process)
        second = receive(process)
        assert process.wait(timeout=3) == 0
        return [first, second]
    finally:
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      publishers.clear()
      if previous is None:
        os.environ.pop('OPENPILOT_PREFIX', None)
      else:
        os.environ['OPENPILOT_PREFIX'] = previous


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--consume', action='store_true')
  parser.add_argument('--output', type=Path)
  args = parser.parse_args()
  if args.consume:
    consume()
    return
  assert args.output is not None
  args.output.mkdir(parents=True, exist_ok=False)
  legacy = scenario(args.output, False)
  fixed = scenario(args.output, True)
  expected_legacy = [{name: 1 if name == 'livePose' else 2 for name in TOPICS}, {'livePose': 2}]
  expected_fixed = [dict.fromkeys(TOPICS, index) for index in (1, 2)]
  report = {'legacy': legacy, 'fixed': fixed, 'legacy_lost_unpolled_frame': legacy == expected_legacy,
            'fixed_preserved_both_frames': fixed == expected_fixed}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  assert report['legacy_lost_unpolled_frame'] and report['fixed_preserved_both_frames'], report
  emit(report)


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp", "pyzmq"]
# ///
# How to run: python rust/tools/check_feedbackd.py --binary TARGET/openpilot-feedbackd --binding PARAMS.so --output EVIDENCE
import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import uuid

import msgq
from openpilot.cereal import car, log
from openpilot.cereal.services import SERVICE_LIST

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/ui/feedback/feedbackd.py'


def original(binding: Path, output: Path) -> None:
  from original_params_binding import load
  from openpilot.cereal import messaging
  module, logs = load(binding, 'ipc://' + str(output / 'log'), output / 'logs')
  params = module.Params(str(output / 'params'))
  params.put_bool('RecordAudioFeedback', True)
  tree = ast.parse(SOURCE.read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom))]
  scope = {'__name__': 'feedback_oracle', 'messaging': messaging, 'Params': lambda: params,
           'cloudlog': logs.cloudlog, 'car': car, 'SAMPLE_RATE': 16000, 'SAMPLE_BUFFER': 800}
  exec(compile(tree, str(SOURCE), 'exec'), scope)
  try:
    scope['main']()
  except KeyboardInterrupt:
    return


def packet(topic: str, valid: bool = True) -> bytes:
  event = log.Event.new_message()
  event.logMonoTime = time.monotonic_ns()
  event.valid = valid
  payload = event.init(topic)
  match topic:
    case 'rawAudioData':
      payload.data = bytes(range(256)) * 6
      payload.sampleRate = 16000
    case 'carState':
      payload.canValid = True
      payload.buttonEvents = [{'type': 'lkas', 'pressed': valid}]
    case 'bookmarkButton':
      pass
    case _:
      raise AssertionError(topic)
  return event.to_bytes()


def scenario(command: list[str], output: Path) -> list[dict[str, int]]:
  output.mkdir(parents=True)
  prefix = 'feedback-' + uuid.uuid4().hex[:16]
  previous = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  publishers = {topic: msgq.pub_sock(topic, SERVICE_LIST[topic].queue_size)
                for topic in ('rawAudioData', 'bookmarkButton', 'carState')}
  subscribers = {}
  environment = os.environ | {'PARAMS_ROOT': str(output / 'params'), 'LOGPRINT': 'info'}
  directory = output / 'params' / prefix
  directory.mkdir(parents=True)
  (directory / 'RecordAudioFeedback').write_text('1')
  rows = []
  with (output / 'stdout.log').open('w') as stdout, (output / 'stderr.log').open('w') as stderr:
    process = subprocess.Popen(command, env=environment, stdout=stdout, stderr=stderr)
    try:
      deadline = time.monotonic() + 8
      while True:
        publishers['rawAudioData'].send(packet('rawAudioData'))
        publishers['carState'].send(packet('carState'))
        time.sleep(.02)
        assert process.poll() is None, (output / 'stderr.log').read_text()
        if all(publishers[topic].all_readers_updated() for topic in ('rawAudioData', 'carState')):
          break
        assert time.monotonic() < deadline, 'feedback subscriptions not ready'
      subscribers = {topic: msgq.sub_sock(topic, timeout=200) for topic in ('userBookmark', 'audioFeedback')}
      for subscriber in subscribers.values():
        assert subscriber.receive(non_blocking=True) is None
      for index in range(8):
        started = time.monotonic_ns()
        publishers['rawAudioData'].send(packet('rawAudioData'))
        publishers['carState'].send(packet('carState', index % 2 == 0))
        if index % 2 == 0:
          publishers['bookmarkButton'].send(packet('bookmarkButton', index % 4 == 0))
          data = subscribers['userBookmark'].receive()
          assert data is not None, (output / 'stderr.log').read_text()
          with log.Event.from_bytes(data) as event:
            assert event.which() == 'userBookmark' and event.valid
            assert started <= event.logMonoTime <= time.monotonic_ns()
          (output / f'bookmark-{index}.capnp').write_bytes(data)
          rows.append({'iteration': index, 'bookmarks': 1})
        else:
          assert subscribers['userBookmark'].receive() is None
          rows.append({'iteration': index, 'bookmarks': 0})
        assert subscribers['audioFeedback'].receive(non_blocking=True) is None
        assert subscribers['userBookmark'].receive(non_blocking=True) is None
      process.send_signal(signal.SIGINT)
      assert process.wait(timeout=3) == 0, (output / 'stderr.log').read_text()
      output_text = (output / 'stdout.log').read_text() + (output / 'stderr.log').read_text()
      assert output_text.count('Bookmark button pressed!') == 4, output_text
    finally:
      if process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      publishers.clear()
      subscribers.clear()
      shutil.rmtree(shm)
      if previous is None:
        os.environ.pop('OPENPILOT_PREFIX', None)
      else:
        os.environ['OPENPILOT_PREFIX'] = previous
  return rows


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--source', action='store_true')
  args = parser.parse_args()
  args.output = args.output.resolve()
  if args.source:
    original(args.binding.resolve(), args.output)
    return
  assert args.binary is not None and args.binary.is_file(), 'native feedback executable missing'
  source = scenario([sys.executable, str(Path(__file__).resolve()), '--source', '--binding', str(args.binding.resolve()),
                     '--output', str(args.output / 'source')], args.output / 'source')
  native = scenario([str(args.binary.resolve())], args.output / 'native')
  assert source == native
  args.output.joinpath('comparison.json').write_text(json.dumps({
    'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'rows': native,
    'audio_recording_disabled': True, 'source_native_equal': True,
  }, indent=2) + '\n')
  print('PASS source/native feedback IPC, validity, logging, disabled recording and shutdown')


if __name__ == '__main__':
  main()

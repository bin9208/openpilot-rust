"""Capture actual original native encoder output for route-logger comparisons."""
from __future__ import annotations

from hashlib import sha256
import json
import os
from pathlib import Path
import select
import shutil
import subprocess

import openpilot.cereal.messaging as messaging
from openpilot.cereal import log


def line(process: subprocess.Popen, timeout: float = 30) -> str:
  if process.stdout is None or not select.select([process.stdout], [], [], timeout)[0]:
    raise TimeoutError('native encoder command acknowledgement')
  return process.stdout.readline().strip()


def capture(binary: Path, output: Path, kind: str) -> list[bytes]:
  output.mkdir(parents=True)
  prefix = f'logger-fixture-{os.getpid()}-{kind}'
  shm = Path('/dev/shm') / ('msgq_' + prefix)
  shm.mkdir()
  params = output / 'params' / prefix
  params.mkdir(parents=True)
  os.environ['OPENPILOT_PREFIX'] = prefix
  service = 'roadEncodeData' if kind == 'road' else 'qRoadEncodeData'
  subscriber = messaging.sub_sock(service, conflate=False)
  environment = dict(os.environ, PARAMS_ROOT=str(output / 'params'))
  records = []
  try:
    with (output / 'stderr.log').open('wb') as stderr:
      process = subprocess.Popen([str(binary), kind], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=stderr, text=True, bufsize=1)
      try:
        assert line(process) == 'ready'
        assert process.stdin is not None
        messaging.drain_sock(subscriber)
        for frame in range(1, 161):
          process.stdin.write(f'frame {frame}\n')
          process.stdin.flush()
          assert line(process) == 'ok'
          records.extend(messaging.drain_sock_raw(subscriber))
        process.stdin.write('quit\n')
        process.stdin.flush()
        assert process.wait(timeout=20) == 0
      finally:
        if process.poll() is None:
          process.kill()
          process.wait()
    assert records, kind
    for index, record in enumerate(records):
      (output / f'{index:04d}.capnp').write_bytes(record)
    metadata = {'producer': str(binary), 'binary_sha256': sha256(binary.read_bytes()).hexdigest(),
                'kind': kind, 'input_frames': 160, 'encoded_packets': len(records),
                'packets_sha256': [sha256(record).hexdigest() for record in records]}
    (output / 'index.json').write_text(json.dumps(metadata, indent=2) + '\n')
    return records
  finally:
    del subscriber
    shutil.rmtree(shm)


def encoded(record: bytes, service: str, segment: int, frame: int, keyframe: bool | None = None) -> bytes:
  with log.Event.from_bytes(record) as source:
    data = getattr(source, source.which()).to_dict()
  data['idx']['segmentNum'] = segment
  data['idx']['frameId'] = frame
  data['idx']['encodeId'] = frame
  data['idx']['segmentId'] = frame
  data['idx']['timestampSof'] = 1_000_000_000 + frame * 50_000_000
  data['idx']['timestampEof'] = data['idx']['timestampSof'] + 1_000_000
  if keyframe is not None:
    data['idx']['flags'] = 8 if keyframe else 0
  message = messaging.new_message(service)
  message.logMonoTime = 4_000_000_000 + frame * 100_000
  message.valid = frame % 2 == 0
  setattr(message, service, data)
  return message.to_bytes()


def event(service: str, sequence: int) -> bytes:
  match service:
    case 'logMessage' | 'errorLogMessage':
      message = messaging.new_message(None)
      setattr(message, service, f'fixture-{sequence:08d}')
    case 'can' | 'sendcan':
      message = messaging.new_message(service, 1)
      getattr(message, service)[0].address = sequence
      getattr(message, service)[0].dat = bytes([sequence % 256, 2, 3])
    case 'rawAudioData':
      message = messaging.new_message(service)
      message.rawAudioData.sampleRate = 16000
      message.rawAudioData.data = b'\x40\x10' * 800
    case _:
      message = messaging.new_message(service)
  message.logMonoTime = 5_000_000_000 + sequence * 50_000_000
  message.valid = sequence % 2 == 0
  return message.to_bytes()

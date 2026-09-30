from __future__ import annotations

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from openpilot.cereal import log
import openpilot.cereal.messaging as messaging
import zstandard
from loggerd_fixtures import encoded, event
from logmessaged_native import Peer as Collector


def run(binary: Path, collector_binary: Path, output: Path, fixture: bytes, original: bool) -> dict:
  collector = Collector(collector_binary, output, original)
  process = None
  publisher = None
  try:
    collector.start()
    params = output / 'params' / collector.prefix
    params.mkdir(parents=True)
    for key, value in {'RecordRoadCam': '0', 'RecordFront': '0', 'RecordAudio': '0', 'RouteCount': '42'}.items():
      (params / key).write_text(value)
    services = ['roadEncodeData', 'userBookmark', 'carState']
    publisher = messaging.PubMaster(services)
    environment = dict(os.environ, PARAMS_ROOT=str(output / 'params'), LOG_ROOT=str(output / 'routes'),
                       LOGGERD_TEST='1', LOGGERD_SEGMENT_LENGTH='60')
    with (output / 'logger.stdout').open('wb') as stdout, (output / 'logger.stderr').open('wb') as stderr:
      process = subprocess.Popen([str(binary)], env=environment, stdout=stdout, stderr=stderr)
    deadline = time.monotonic() + 20
    while not (params / 'CurrentRoute').exists():
      assert process.poll() is None, (output / 'logger.stderr').read_text()
      assert time.monotonic() < deadline
      time.sleep(.005)
    route = (params / 'CurrentRoute').read_text()
    inputs = [
      ('roadEncodeData', encoded(fixture, 'roadEncodeData', 70, 0, False)),
      ('roadEncodeData', encoded(fixture, 'roadEncodeData', 70, 1, True)),
      ('userBookmark', event('userBookmark', 0)),
      ('roadEncodeData', encoded(fixture, 'roadEncodeData', 0, 2, True)),
    ]
    for service, data in inputs:
      publisher.send(service, data)
      assert publisher.wait_for_readers_to_update(service, timeout=10, dt=.001)
    published = []
    while True:
      packet = collector.subscribers['logMessage'].receive()
      assert packet is not None, 'missing logger diagnostic publication'
      with log.Event.from_bytes(packet) as message:
        assert message.which() == 'logMessage' and message.valid
        raw = message.logMessage
        record = json.loads(raw)
      published.append(raw)
      if 'encoderd packet has a older segment!!!' in record['msg']:
        break
    error_packet = collector.subscribers['errorLogMessage'].receive()
    assert error_packet is not None
    with log.Event.from_bytes(error_packet) as message:
      assert message.valid and message.errorLogMessage == published[-1]
    collector.socket.send(b'\x0a{"msg":"pipeline-drain"}')
    marker = collector.subscribers['logMessage'].receive()
    assert marker is not None
    with log.Event.from_bytes(marker) as message:
      assert json.loads(message.logMessage)['msg'] == 'pipeline-drain'
    time.sleep(.2)
    process.send_signal(signal.SIGTERM)
    assert process.wait(timeout=20) == 0, (output / 'logger.stderr').read_text()
    collector.stop()
    segment = output / 'routes' / (route + '--0')
    assert not list(segment.glob('*.lock'))
    with (segment / 'rlog.zst').open('rb') as compressed:
      raw = zstandard.ZstdDecompressor().stream_reader(compressed).read()
    (output / 'rlog.capnp').write_bytes(raw)
    routed = {'logMessage': [], 'errorLogMessage': []}
    for message in log.Event.read_multiple_bytes(raw):
      if message.which() in routed:
        routed[message.which()].append(getattr(message, message.which()))
    expected = [record for record in published if any(phrase in json.loads(record)['msg'] for phrase in
                ('dropped 1 non iframe packets', 'preserving ', 'encoderd packet has a older segment!!!'))]
    assert len(expected) == 3
    assert all(record in routed['logMessage'] for record in expected)
    assert routed['errorLogMessage'] == [published[-1]]
    assert any(json.loads(record)['msg'] == 'pipeline-drain' for record in routed['logMessage'])
    disk = '\n'.join(path.read_text() for path in collector.root.glob('swaglog.*'))
    assert all(json.loads(record)['msg'] in disk for record in expected)
    assert 'pipeline-drain' not in disk
    result = {'logger_sha256': sha256(binary.read_bytes()).hexdigest(), 'collector': 'original' if original else 'rust',
              'published_records': len(published), 'expected_warning_error_records': 3, 'routed_error_records': 1,
              'native_cereal_and_disk_verified': True}
    (output / 'publications.json').write_text(json.dumps(published, indent=2) + '\n')
    (output / 'routed.json').write_text(json.dumps(routed, indent=2) + '\n')
    return result
  finally:
    if process is not None and process.poll() is None:
      process.kill()
      process.wait(timeout=10)
    publisher = None
    collector.close()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--original', type=Path, required=True)
  parser.add_argument('--collector', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  results = {side: run(binary.resolve(), args.collector.resolve(), output / side, args.fixture.read_bytes(), side == 'original')
             for side, binary in [('original', args.original), ('rust', args.binary)]}
  (output / 'report.json').write_text(json.dumps(results, indent=2) + '\n')
  print(json.dumps(results))


if __name__ == '__main__':
  main()

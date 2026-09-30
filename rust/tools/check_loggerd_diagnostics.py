from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import time

from openpilot.cereal import log
from loggerd_diagnostics import compare, records
from loggerd_fixtures import encoded, event
from loggerd_peer import Peer, PeerSettings
from loggerd_scenarios import ENCODERS, queue_restart


def throughput(binary: Path, output: Path, fixtures: dict[str, list[bytes]]) -> dict:
  peer = Peer(binary, output, PeerSettings(tuple(ENCODERS) + ('can',)))
  try:
    queue_restart(peer, fixtures)
    while len(peer.inputs) < 10000:
      peer.send('can', event('can', len(peer.inputs)))
    logs = peer.stop()
    messages, _ = records(output)
    counters = [re.fullmatch(r'10000 messages, ([0-9.]+) msg/sec, ([0-9.]+) KB/sec', message) for _, message in messages]
    counters = [match for match in counters if match]
    assert len(counters) == 1
    message_rate, byte_rate = map(float, counters[0].groups())
    assert message_rate > 0 and byte_rate > 0
    total = 0
    for part in range(len(logs)):
      raw = (peer.segment(part) / 'rlog.capnp').read_bytes()
      for packet in log.Event.read_multiple_bytes(raw):
        if packet.which() not in ('initData', 'sentinel'):
          total += len(packet.as_builder().to_bytes())
    total -= len(peer.inputs[-1][1])
    bytes_per_message = total * .001 / 10000
    assert abs(byte_rate - message_rate * bytes_per_message) <= .005 * (1 + bytes_per_message), (total, message_rate, byte_rate)
    return {'input_messages_at_report': 10000, 'logged_bytes_at_report': total,
            'message_rate': message_rate, 'kilobyte_rate': byte_rate,
            'rate_rounding': 'two decimal places', 'queued_index_packets_included': True}
  finally:
    peer.cleanup()


def suppression(binary: Path, output: Path, record: bytes) -> Path:
  peer = Peer(binary, output, PeerSettings(('qRoadEncodeData', 'rawAudioData'), audio=True))
  try:
    for frame in range(205):
      peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, frame, True))
    time.sleep(.12)
    peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, 205, True))
    peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, 206, True))
    peer.send('rawAudioData', event('rawAudioData', 0))
    peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, 207, True))
    peer.stop()
    messages, _ = records(output)
    assert (30, 'cloudlog: 2 messages suppressed') in messages
    assert sum('dropping frame waiting for audio' in message for _, message in messages) == 4
    return output
  finally:
    peer.cleanup()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--original', required=True, type=Path)
  parser.add_argument('--fixtures', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  fixtures = {kind: [path.read_bytes() for path in sorted((args.fixtures / kind).glob('*.capnp'))] for kind in ('road', 'qroad')}
  summary = {}
  for side, binary in [('original', args.original.resolve()), ('rust', args.binary.resolve())]:
    summary[side] = throughput(binary, output / ('throughput-' + side), fixtures)
    suppression(binary, output / ('suppression-' + side), fixtures['qroad'][0])
  assert summary['original']['logged_bytes_at_report'] == summary['rust']['logged_bytes_at_report']
  summary['suppression'] = compare(output / 'suppression-original', output / 'suppression-rust')
  (output / 'report.json').write_text(json.dumps(summary, indent=2) + '\n')
  print(json.dumps(summary))


if __name__ == '__main__':
  main()

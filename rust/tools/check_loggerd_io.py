import argparse
import json
from pathlib import Path

from openpilot.cereal import messaging
from loggerd_peer import Peer, PeerSettings
from loggerd_fixtures import encoded
from loggerd_diagnostics import compare as compare_diagnostics, records
from loggerd_scenarios import ENCODERS


def raw_write_failure(binary: Path, root: Path) -> dict:
  trace = root / 'writes.log'
  peer = Peer(binary, root, PeerSettings(('roadEncodeData',), runner=(
    '/usr/bin/strace', '-D', '-f', '-yy', '-e', 'trace=write,close', '-o', str(trace))))
  try:
    (peer.segment(0) / 'fcamera.hevc').symlink_to('/dev/full')
    for frame in range(3):
      message = messaging.new_message('roadEncodeData')
      data = message.roadEncodeData
      data.width, data.height = 160, 120
      data.idx.type, data.idx.flags = 'fullHEVC', 8
      data.idx.frameId = frame
      data.idx.timestampEof = (frame + 1) * 50_000_000
      data.header, data.data = b'header', b'X' * 8192
      peer.send('roadEncodeData', message.to_bytes())
    logs = peer.stop()
    indices = [entry['roadEncodeIdx'] for entry in logs[0]['rlog'] if 'roadEncodeIdx' in entry]
    assert [entry['frameId'] for entry in indices] == [0, 1, 2], indices
    assert not (peer.segment(0) / 'fcamera.hevc.lock').exists()
    failed = [line for line in trace.read_text().splitlines() if '</dev/full' in line and 'ENOSPC' in line]
    assert failed, trace.read_text()
    return {'exit': 0, 'indices': indices, 'video_lock_removed': True, 'enospc_observed': True}
  finally:
    peer.cleanup()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--original', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--fixtures', type=Path)
  parser.add_argument('--remux-only', action='store_true')
  args = parser.parse_args()
  output = args.output.resolve()
  result = {'result': 'pass'}
  if args.remux_only and not args.fixtures:
    parser.error('--remux-only requires --fixtures')
  if not args.remux_only:
    original = raw_write_failure(args.original.resolve(), output / 'original')
    rust = raw_write_failure(args.binary.resolve(), output / 'rust')
    assert original == rust, (original, rust)
    diagnostics = compare_diagnostics(output / 'original', output / 'rust')
    result.update(original=original, rust=rust, diagnostics=diagnostics)
  if args.fixtures:
    result['remux'] = {}
    for side, binary in [('original', args.original), ('rust', args.binary)]:
      result['remux'][side] = remux_write_failure(binary.resolve(), output / ('remux-' + side), args.fixtures)
    assert result['remux']['original'] == result['remux']['rust']
    result['remux']['diagnostics'] = compare_diagnostics(output / 'remux-original', output / 'remux-rust')
  (output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
  print('video write failures preserve original logging/index/close behavior')


def remux_write_failure(binary: Path, root: Path, fixtures: Path) -> dict:
  peer = Peer(binary, root, PeerSettings(tuple(ENCODERS), parameters=(('RecordRoadCam', '0'), ('RecordFront', '0'))))
  try:
    (peer.segment(0) / 'qcamera.ts').symlink_to('/dev/full')
    packets = sorted((fixtures / 'qroad').glob('*.capnp'))
    road = (fixtures / 'road/0000.capnp').read_bytes()
    for service in ENCODERS[:-1]:
      peer.send(service, encoded(road, service, 0, 0, True))
    for frame, path in enumerate(packets * 4):
      peer.send('qRoadEncodeData', encoded(path.read_bytes(), 'qRoadEncodeData', 0, frame))
    for service in ENCODERS[:-1]:
      peer.send(service, encoded(road, service, 1, 1, True))
    for frame in (len(packets) * 4, len(packets) * 4 + 1):
      peer.send('qRoadEncodeData', encoded(packets[0].read_bytes(), 'qRoadEncodeData', 1, frame, True))
    logs = peer.stop()
    indices = [entry['qRoadEncodeIdx']['frameId'] for entry in logs[0]['rlog'] if 'qRoadEncodeIdx' in entry]
    assert indices == list(range(len(packets) * 4))
    messages, _ = records(root)
    assert any('ts encoder write issue len:' in message for _, message in messages)
    assert (40, 'av_write_trailer failed -28') in messages
    assert (40, 'avio_closep failed -28') in messages
    return {'indices': len(indices), 'exit': 0, 'packet_warning': True, 'trailer_enospc': True, 'avio_close_enospc': True}
  finally:
    peer.cleanup()


if __name__ == '__main__':
  main()

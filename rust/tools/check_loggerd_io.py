import argparse
import json
from pathlib import Path

from openpilot.cereal import messaging
from loggerd_peer import Peer, PeerSettings


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
  args = parser.parse_args()
  output = args.output.resolve()
  original = raw_write_failure(args.original.resolve(), output / 'original')
  rust = raw_write_failure(args.binary.resolve(), output / 'rust')
  assert original == rust, (original, rust)
  (output / 'report.json').write_text(json.dumps({'result': 'pass', 'original': original, 'rust': rust}, indent=2) + '\n')
  print('raw video write failure preserves original logging/index/close behavior')


if __name__ == '__main__':
  main()

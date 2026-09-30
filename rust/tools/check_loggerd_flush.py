import argparse
import json
from pathlib import Path
import time

from openpilot.cereal import messaging
from loggerd_peer import Peer, PeerSettings


def run(binary: Path, root: Path) -> dict:
  trace = root / 'sync-syscalls.log'
  settings = PeerSettings(('roadEncodeData',), runner=(
    '/usr/bin/strace', '-D', '-f', '-yy', '-e', 'trace=fsync,fdatasync', '-o', str(trace)))
  peer = Peer(binary, root, settings)
  try:
    message = messaging.new_message('roadEncodeData')
    message.roadEncodeData.width = 160
    message.roadEncodeData.height = 120
    message.roadEncodeData.idx.type = 'fullHEVC'
    message.roadEncodeData.idx.flags = 8
    message.roadEncodeData.idx.timestampEof = 1_000_000
    message.roadEncodeData.header = b'fixture-header'
    message.roadEncodeData.data = b'fixture-payload'
    peer.send('roadEncodeData', message.to_bytes())
    peer.stop()
    deadline = time.monotonic() + 2
    while 'exited with 0' not in trace.read_text() and time.monotonic() < deadline:
      time.sleep(.01)
    lines = trace.read_text().splitlines()
    assert any('fsync(' in line and '/params/' in line for line in lines), lines
    log_syncs = [line for line in lines if str(root / 'logs') in line and ('fsync(' in line or 'fdatasync(' in line)]
    assert (peer.segment(0) / 'fcamera.hevc').read_bytes() == b'fixture-headerfixture-payload'
    return {'log_syncs': log_syncs, 'route_finalized': True, 'raw_bytes_exact': True}
  finally:
    peer.cleanup()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--original', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  source = run(args.original.resolve(), output / 'original')
  candidate = run(args.binary.resolve(), output / 'rust')
  report = {'original': source, 'rust': candidate}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  assert source['log_syncs'] == candidate['log_syncs'] == [], report
  print(json.dumps(report))


if __name__ == '__main__':
  main()

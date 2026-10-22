import argparse
import json
from pathlib import Path
import re
import signal
import subprocess

from openpilot.cereal import messaging
from loggerd_peer import Peer, PeerSettings
from loggerd_fixtures import encoded
from loggerd_diagnostics import PROVENANCE, compare as compare_diagnostics, records
from loggerd_scenarios import ENCODERS
from loggerd_validation import metadata, normalize


def raw_write_failure(binary: Path, root: Path, *, candidate: bool, phase: str, fault_library: Path,
                      uploader: Path | None = None, deleter: Path | None = None) -> dict:
  trace = root / 'writes.log'
  runner = ['/usr/bin/strace', '-D', '-f', '-yy', '-e', 'trace=write,close', '-o', str(trace)]
  if phase in ('close', 'eintr'):
    runner += ['/usr/bin/env', f'LD_PRELOAD={fault_library}', f'LOGGERD_RAW_FAULT={phase}']
  peer = Peer(binary, root, PeerSettings(('roadEncodeData', 'wideRoadEncodeData'), runner=tuple(runner)))
  try:
    if phase in ('write', 'flush'):
      (peer.segment(0) / 'fcamera.hevc').symlink_to('/dev/full')
    payload = b'X' * (8192 if phase in ('write', 'eintr') else 17)
    for frame in range(3):
      message = messaging.new_message('roadEncodeData')
      message.logMonoTime = (frame + 1) * 50_000_000
      data = message.roadEncodeData
      data.width, data.height = 160, 120
      data.idx.type, data.idx.flags = 'fullHEVC', 8
      data.idx.frameId = frame
      data.idx.timestampEof = (frame + 1) * 50_000_000
      data.header, data.data = b'header', payload
      peer.send('roadEncodeData', message.to_bytes())
    # wideRoad sorts after road in the daemon's shutdown loop. It must still
    # close normally after the failed raw road video has reported its error.
    message = messaging.new_message('wideRoadEncodeData')
    message.logMonoTime = 200_000_000
    data = message.wideRoadEncodeData
    data.width, data.height = 160, 120
    data.idx.type, data.idx.flags = 'fullHEVC', 8
    data.idx.timestampEof = 200_000_000
    data.header, data.data = b'wide-header', b'wide-payload'
    peer.send('wideRoadEncodeData', message.to_bytes())
    peer.barrier()
    if phase == 'flush':
      assert 'ENOSPC' not in trace.read_text(), 'small packets must remain buffered until shutdown'
    peer.process.send_signal(signal.SIGTERM)
    status = peer.process.wait(timeout=20)
    assert status == 0, (root, status, (root / 'stderr.log').read_text())
    assert not (peer.segment(0) / 'fcamera.hevc.lock').exists()
    assert (peer.segment(0) / 'fcamera.hevc.failed').is_file() == candidate
    assert not (peer.segment(0) / 'rlog.lock').exists()
    assert not (peer.segment(0) / 'ecamera.hevc.lock').exists()
    assert (peer.segment(0) / 'ecamera.hevc').read_bytes() == b'wide-headerwide-payload'
    logs = peer.logs()
    for name in ('rlog', 'qlog'):
      metadata(logs[0][name], candidate)
      indices = [entry['roadEncodeIdx'] for entry in logs[0][name] if 'roadEncodeIdx' in entry]
      assert [entry['frameId'] for entry in indices] == [0, 1, 2], (name, indices)
      assert logs[0][name][-1]['sentinel'] == {'type': 'endOfRoute', 'signal': signal.SIGTERM}
    peer.cleanup()
    failed = [line for line in trace.read_text().splitlines() if '</dev/full' in line and 'ENOSPC' in line]
    if phase == 'close':
      assert (root / 'stderr.log').read_text().count('loggerd-io-fault: fclose ENOSPC') == 1
      assert (peer.segment(0) / 'fcamera.hevc').read_bytes() == b'header' + payload * 3
    elif phase == 'eintr':
      assert (root / 'stderr.log').read_text().count('loggerd-io-fault: full-count ferror EINTR') == 1
      # A real failed stdio flush lost the buffered header. Full payload counts
      # and later successful writes cannot establish that the video is complete.
      assert (peer.segment(0) / 'fcamera.hevc').read_bytes() == payload * 3
    else:
      assert failed, trace.read_text()
      closes = [line for line in trace.read_text().splitlines() if 'close(' in line and '</dev/full' in line]
      assert len(closes) == 1, closes
    messages, _ = records(root)
    errno = 4 if phase == 'eintr' else 28
    write_errors = [row for row in messages if row == (40, f'failed to write file.errno={errno}')]
    finish_error = (40, f'raw video incomplete; marked <root>/logs/<route>--0/fcamera.hevc.failed errno={errno}')
    if candidate:
      assert messages.count(finish_error) == 1, messages
      if phase in ('write', 'eintr'):
        assert len(write_errors) == 3, messages
      else:
        assert not write_errors, 'delayed failure must not be attributed to a successful buffered write'
      assert 'loggerd: fatal:' not in (root / 'stderr.log').read_text()
    else:
      assert finish_error not in messages
    policy = None
    if candidate:
      assert uploader is not None and deleter is not None
      policy = completed_failure_policy(peer, uploader, deleter)
    return {'exit': status, 'indices': indices, 'video_failed_marker': candidate,
            'io_fault_observed': phase, 'rlog_qlog_finalized': True, 'storage_policy': policy}
  finally:
    peer.cleanup()


def completed_failure_policy(peer: Peer, uploader: Path, deleter: Path) -> dict:
  request = {'root': str(peer.root / 'logs')}
  completed = subprocess.run([str(uploader)], input=json.dumps(request) + '\n',
                             text=True, capture_output=True, check=True, timeout=20)
  scanned = json.loads(completed.stdout)
  assert {row[0] for row in scanned['files']} == {'qlog.zst', 'rlog.zst', 'ecamera.hevc', 'rlog.capnp', 'qlog.capnp'}, scanned
  assert scanned['next'][0] == 'qlog.zst', scanned
  request = {'op': 'cycle', 'bytes': '0', 'percent': '0'}
  completed = subprocess.run([str(deleter), str(peer.root / 'logs')], input=json.dumps(request) + '\n',
                             text=True, capture_output=True, check=True, timeout=20)
  deleted = json.loads(completed.stdout)
  assert deleted['deleted'] == peer.segment(0).name.encode().hex(), deleted
  assert not peer.segment(0).exists()
  return {'failed_video_excluded': True, 'healthy_files_eligible': True, 'segment_reclaimed': True}


def compare_raw_failure(source: Path, candidate: Path, phase: str) -> dict:
  # Issue #54 deliberately changes only raw I/O diagnostics and its per-file
  # completed-failure marker. Keep all ordinary diagnostic records/context and complete
  # rlog/qlog records exact, with the same provenance/time normalizations as QA.
  original, original_context = records(source)
  rust, rust_context = records(candidate)
  assert rust_context['runtime_language'] == 'rust'
  assert re.fullmatch(r'[0-9a-f]{40}', rust_context['source_commit'])
  assert rust_context['source_tree'] in {'clean', 'dirty'}
  assert {key: value for key, value in rust_context.items() if key not in PROVENANCE} == original_context
  errno = 4 if phase == 'eintr' else 28
  intentional = {(40, f'failed to write file.errno={errno}'),
                 (40, f'raw video incomplete; marked <root>/logs/<route>--0/fcamera.hevc.failed errno={errno}')}
  assert [row for row in original if row not in intentional] == [row for row in rust if row not in intentional]
  # Decode from the saved native full-schema capture; convert byte wrappers back
  # before using the shared normalizer (JSON is otherwise a lossless container).
  def decode(path):
    return json.loads((path / 'decoded.json').read_text(),
                      object_hook=lambda row: bytes.fromhex(row['hex']) if set(row) == {'hex'} else row)
  source_log, rust_log = decode(source), decode(candidate)
  assert len(source_log) == len(rust_log) == 1
  for name in ('rlog', 'qlog'):
    assert normalize(source_log[0][name]) == normalize(rust_log[0][name]), name
  assert [name for name in rust_log[0]['files'] if name != 'fcamera.hevc.failed'] == source_log[0]['files']
  return {'logs_and_indices': 'exact', 'unaffected_diagnostics_and_context': 'exact',
          'issue_54_exception': f'raw errno={errno} diagnostics and per-file completed-failure marker only'}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--original', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--fixtures', type=Path)
  parser.add_argument('--remux-only', action='store_true')
  parser.add_argument('--uploader', type=Path, help='hosted uploader_trace example for actual recorded-file scan')
  parser.add_argument('--deleter', type=Path, help='hosted deleter_trace example for actual segment reclamation')
  parser.add_argument('--raw-phase', action='append', choices=('write', 'flush', 'close', 'eintr'),
                      help='select a focused raw failure case; default checks all four')
  args = parser.parse_args()
  output = args.output.resolve()
  result = {'result': 'pass'}
  if args.remux_only and not args.fixtures:
    parser.error('--remux-only requires --fixtures')
  if not args.remux_only:
    if args.uploader is None or args.deleter is None:
      parser.error('raw failure checks require --uploader and --deleter storage-policy probes')
    output.mkdir(parents=True, exist_ok=True)
    fault_library = output / 'raw-video-fault.so'
    subprocess.run(['cc', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror',
                    str(Path(__file__).resolve().parents[1] / 'crates/loggerd/tests/raw_video_fault.c'),
                    '-ldl', '-o', str(fault_library)], check=True)
    result['raw'] = {}
    for phase in args.raw_phase or ('write', 'flush', 'close', 'eintr'):
      source, candidate = output / ('original-' + phase), output / ('rust-' + phase)
      original = raw_write_failure(args.original.resolve(), source, candidate=False, phase=phase, fault_library=fault_library)
      rust = raw_write_failure(args.binary.resolve(), candidate, candidate=True, phase=phase, fault_library=fault_library,
                               uploader=args.uploader.resolve(), deleter=args.deleter.resolve())
      assert original['indices'] == rust['indices']
      result['raw'][phase] = {'original': original, 'rust': rust, 'comparison': compare_raw_failure(source, candidate, phase)}
  if args.fixtures:
    result['remux'] = {}
    for side, binary in [('original', args.original), ('rust', args.binary)]:
      result['remux'][side] = remux_write_failure(binary.resolve(), output / ('remux-' + side), args.fixtures)
    assert result['remux']['original'] == result['remux']['rust']
    result['remux']['diagnostics'] = compare_diagnostics(output / 'remux-original', output / 'remux-rust')
  (output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
  print('raw failures exclude damaged video, finalize healthy files and permit reclamation; remux comparisons remain exact')


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

"""Real-time fallback, service-selection, missing-audio and shutdown/error cases."""
from __future__ import annotations

import json
from pathlib import Path
import resource
import signal
import time

from loggerd_fixtures import encoded, event
from loggerd_peer import Peer, PeerSettings
from loggerd_scenarios import ENCODERS, OFFSETS
from loggerd_validation import compare, normalize


def fallbacks(original: Path, binary: Path, output: Path, road: bytes) -> dict:
  peers = []
  results = {}
  try:
    for kind in ('no-camera', 'stuck-encoder'):
      for implementation, executable in (('original', original), ('rust', binary)):
        settings = PeerSettings(('deviceState', 'roadEncodeData'), test=False, parameters=(('RecordRoadCam', '0'),))
        peer = Peer(executable, output / f'{kind}-{implementation}', settings)
        peers.append((kind, implementation, peer, peer.started))
    rotated = {}
    sequence = 0
    while len(rotated) < len(peers):
      sequence += 1
      for kind, implementation, peer, started in peers:
        identity = kind + '-' + implementation
        if identity in rotated:
          continue
        if kind == 'stuck-encoder':
          peer.send('roadEncodeData', encoded(road, 'roadEncodeData', 0, sequence))
        else:
          peer.send('deviceState', event('deviceState', sequence))
        peer.barrier()
        elapsed = time.monotonic() - started
        if peer.segment(1).is_dir():
          minimum = 60 if kind == 'no-camera' else 72
          assert minimum <= elapsed <= minimum + 2, (identity, elapsed)
          rotated[identity] = elapsed
        assert elapsed < 75, identity
      time.sleep(.1)
    for kind, implementation, peer, _ in peers:
      logs = peer.stop()
      assert len(logs) == 2
      assert logs[0]['rlog'][-1]['sentinel']['type'] == 'endOfSegment'
      assert logs[1]['rlog'][1]['sentinel']['type'] == 'startOfSegment'
      results[kind + '-' + implementation] = {'rotation_seconds': rotated[kind + '-' + implementation], 'segments': 2}
    (output / 'fallbacks.json').write_text(json.dumps(results, indent=2) + '\n')
    return results
  except BaseException:
    for _, _, peer, _ in peers:
      if peer.process.poll() is None:
        peer.cleanup()
    raise


def missing_audio(binary: Path, root: Path, record: bytes) -> tuple[Peer, list[dict]]:
  peer = Peer(binary, root, PeerSettings(('qRoadEncodeData',), audio=True))
  try:
    for sequence in range(205):
      peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, sequence, True))
    peer.barrier()
    peer.process.send_signal(signal.SIGTERM)
    exit_status = peer.process.wait(timeout=20)
    assert exit_status != 0
    assert (peer.segment(0) / 'qcamera.ts.lock').is_file()
    assert not (peer.segment(0) / 'rlog.lock').exists()
    logs = peer.logs()
    assert not any('qRoadEncodeIdx' in message for message in logs[0]['rlog'])
    assert (peer.segment(0) / 'qcamera.ts').stat().st_size == 0
    (root / 'shutdown.json').write_text(json.dumps({'exit_status': exit_status, 'video_bytes': 0, 'retained_lock': 'qcamera.ts.lock',
                                                  'rlog_finalized': True, 'qlog_finalized': True}, indent=2) + '\n')
    peer.cleanup()
    return peer, logs
  except BaseException:
    peer.cleanup()
    raise


def disabled(binary: Path, root: Path, fixtures: dict[str, list[bytes]]) -> tuple[Peer, list[dict]]:
  settings = PeerSettings(tuple(ENCODERS), parameters=(('RecordRoadCam', '0'), ('RecordFront', '0')))
  peer = Peer(binary, root, settings)
  try:
    for part in range(2):
      for frame in range(3):
        for service, offset in zip(ENCODERS, OFFSETS, strict=True):
          record = fixtures['qroad' if service == 'qRoadEncodeData' else 'road'][frame]
          keyframe = None if service == 'qRoadEncodeData' else not (part == 1 and frame == 0)
          peer.send(service, encoded(record, service, offset + part, part * 10 + frame, keyframe))
    logs = peer.stop()
    assert len(logs) == 2
    for part in range(2):
      assert not list(peer.segment(part).glob('*.hevc'))
      assert sum('roadEncodeIdx' in message for message in logs[part]['rlog']) == 3
    return peer, logs
  except BaseException:
    peer.cleanup()
    raise


def selection(binary: Path, root: Path) -> tuple[Peer, list[dict]]:
  skipped = ('livestreamRoadEncodeData', 'livestreamWideRoadEncodeData', 'livestreamDriverEncodeData', 'youtubeRoadEncodeData', 'rawAudioData')
  peer = Peer(binary, root, PeerSettings(skipped))
  try:
    for service in skipped:
      peer.publisher.send(service, b'malformed-unsubscribed-data')
    peer.barrier()
    logs = peer.stop()
    assert len(normalize(logs[0]['rlog'])) == 3
    return peer, logs
  except BaseException:
    peer.cleanup()
    raise


def signals(binary: Path, output: Path) -> dict:
  results = {}
  for sent in (signal.SIGINT, signal.SIGTERM, signal.SIGPWR):
    peer = Peer(binary, output / ('signal-' + str(sent)), PeerSettings(('carState',)))
    try:
      peer.send('carState', event('carState', 1))
      logs = peer.stop(sent)
      assert logs[0]['rlog'][-1]['sentinel'] == {'type': 'endOfRoute', 'signal': sent}
      results[str(sent)] = {'exit_status': 0, 'sentinel_signal': sent, 'locks': 0}
    except BaseException:
      peer.cleanup()
      raise
  return results


def malformed(binary: Path, root: Path) -> dict:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  peer = Peer(binary, root, PeerSettings(('roadEncodeData',)))
  try:
    peer.publisher.send('roadEncodeData', b'\xff' * 64)
    assert peer.process.wait(timeout=20) != 0
    assert (peer.segment(0) / 'rlog.lock').is_file()
    result = {'exit_status': peer.process.returncode, 'incomplete_lock_retained': True}
    (root / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    return result
  finally:
    peer.cleanup()


def run_edges(original: Path, binary: Path, output: Path, fixtures: dict[str, list[bytes]]) -> dict:
  results = {}
  for scenario, run in [('disabled', lambda executable, path: disabled(executable, path, fixtures)),
                        ('selection', selection)]:
    results[scenario] = compare(run(original, output / (scenario + '-original')), run(binary, output / (scenario + '-rust')))
  source = missing_audio(original, output / 'missing-audio-original', fixtures['qroad'][0])
  candidate = missing_audio(binary, output / 'missing-audio-rust', fixtures['qroad'][0])
  assert normalize(source[1][0]['rlog']) == normalize(candidate[1][0]['rlog'])
  results['missing-audio'] = {'input_packets': 205, 'published_indices': 0, 'video_bytes': 0,
                            'original': json.loads((source[0].root / 'shutdown.json').read_text()),
                            'rust': json.loads((candidate[0].root / 'shutdown.json').read_text())}
  results['signals-original'] = signals(original, output / 'original')
  results['signals-rust'] = signals(binary, output / 'rust')
  results['malformed-original'] = malformed(original, output / 'malformed-original')
  results['malformed-rust'] = malformed(binary, output / 'malformed-rust')
  (output / 'edges.json').write_text(json.dumps(results, indent=2) + '\n')
  return results

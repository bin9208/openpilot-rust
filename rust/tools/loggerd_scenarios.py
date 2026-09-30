"""Source-derived route logger input scenarios over the original native transport."""
from __future__ import annotations

import os
from pathlib import Path
import signal

from loggerd_fixtures import encoded, event
from loggerd_peer import Peer, PeerSettings

ENCODERS = ['roadEncodeData', 'wideRoadEncodeData', 'driverEncodeData', 'qRoadEncodeData']
OFFSETS = [70, 10, 0, 29]
ORDINARY = ['can', 'carState', 'deviceState', 'controlsState', 'modelV2', 'livePose', 'errorLogMessage', 'logMessage']


def ordinary(peer: Peer) -> None:
  for sequence in range(101):
    for service in ORDINARY:
      peer.send(service, event(service, sequence))


def burst(peer: Peer) -> None:
  records = [event('can', sequence) for sequence in range(1000)]
  for record in records:
    peer.publisher.send('can', record)
    peer.inputs.append(('can', record))
  assert peer.publisher.wait_for_readers_to_update('can', timeout=10, dt=.001)
  peer.barrier()


def fairness(peer: Peer) -> None:
  peer.process.send_signal(signal.SIGSTOP)
  _, status = os.waitpid(peer.process.pid, os.WUNTRACED)
  assert os.WIFSTOPPED(status)
  for service, count in [('can', 500), ('carState', 3), ('deviceState', 1)]:
    for sequence in range(count):
      raw = event(service, sequence)
      peer.publisher.send(service, raw)
      peer.inputs.append((service, raw))
  peer.process.send_signal(signal.SIGCONT)
  for service in ('can', 'carState', 'deviceState'):
    assert peer.publisher.wait_for_readers_to_update(service, timeout=10, dt=.001)
  peer.barrier()


def video(peer: Peer, fixtures: dict[str, list[bytes]]) -> None:
  for service, offset in zip(ENCODERS, OFFSETS, strict=True):
    record = fixtures['qroad' if service == 'qRoadEncodeData' else 'road'][0]
    peer.send(service, encoded(record, service, offset, 999, False))
  for part in range(3):
    for frame in range(30):
      for service, offset in zip(ENCODERS, OFFSETS, strict=True):
        record = fixtures['qroad' if service == 'qRoadEncodeData' else 'road'][frame]
        peer.send(service, encoded(record, service, offset + part, part * 1000 + frame))
      peer.send('carState', event('carState', part * 1000 + frame))
    peer.send('userBookmark', event('userBookmark', part))
    peer.send('audioFeedback', event('audioFeedback', part))
    peer.barrier()
    assert os.getxattr(peer.segment(part), 'user.preserve') == b'1'
  assert (peer.params / 'AthenadRecentlyViewedRoutes').read_text() == 'old-route' + (',' + peer.route) * 3


def audio(peer: Peer, fixtures: dict[str, list[bytes]]) -> None:
  packets = fixtures['qroad']
  peer.send('qRoadEncodeData', encoded(packets[0], 'qRoadEncodeData', 29, 0))
  for sequence in range(225):
    peer.send('rawAudioData', event('rawAudioData', sequence))
  for frame in range(1, 30):
    peer.send('qRoadEncodeData', encoded(packets[frame], 'qRoadEncodeData', 29, frame))
    peer.send('rawAudioData', event('rawAudioData', 225 + frame))


def audio_queue(peer: Peer, fixtures: dict[str, list[bytes]]) -> None:
  record = fixtures['qroad'][0]
  for frame in range(205):
    peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, frame, True))
  peer.send('rawAudioData', event('rawAudioData', 0))
  peer.send('qRoadEncodeData', encoded(record, 'qRoadEncodeData', 0, 206, True))


def queue_restart(peer: Peer, fixtures: dict[str, list[bytes]]) -> None:
  for service, offset in zip(ENCODERS, OFFSETS, strict=True):
    record = fixtures['qroad' if service == 'qRoadEncodeData' else 'road'][0]
    peer.send(service, encoded(record, service, offset, 0))
  for frame in range(205):
    peer.send('roadEncodeData', encoded(fixtures['road'][frame % 160], 'roadEncodeData', 71, frame + 1000))
  for service, offset in zip(ENCODERS[1:], OFFSETS[1:], strict=True):
    record = fixtures['qroad' if service == 'qRoadEncodeData' else 'road'][0]
    peer.send(service, encoded(record, service, offset + 1, 1000))
  peer.send('roadEncodeData', encoded(fixtures['road'][0], 'roadEncodeData', 71, 2000))
  peer.send('roadEncodeData', encoded(fixtures['road'][0], 'roadEncodeData', 0, 2001))
  peer.send('roadEncodeData', encoded(fixtures['road'][1], 'roadEncodeData', 0, 2002))
  peer.barrier()
  assert peer.segment(1).is_dir()


def run(binary: Path, root: Path, scenario: str, fixtures: dict[str, list[bytes]], runner: tuple[str, ...] = ()) -> tuple[Peer, list[dict]]:
  services = list(set(ORDINARY + ENCODERS + ['userBookmark', 'audioFeedback', 'rawAudioData']))
  peer = Peer(binary, root, PeerSettings(tuple(services), audio=scenario in ('audio', 'audio-queue'), runner=runner))
  try:
    match scenario:
      case 'ordinary':
        ordinary(peer)
      case 'burst':
        burst(peer)
      case 'fairness':
        fairness(peer)
      case 'video':
        video(peer, fixtures)
      case 'audio':
        audio(peer, fixtures)
      case 'audio-queue':
        audio_queue(peer, fixtures)
      case 'queue-restart':
        queue_restart(peer, fixtures)
      case _:
        raise ValueError(scenario)
    logs = peer.stop()
    if scenario == 'audio-queue':
      ids = [message['qRoadEncodeIdx']['frameId'] for message in logs[0]['rlog'] if 'qRoadEncodeIdx' in message]
      assert ids == [*range(201), 206]
    if scenario == 'queue-restart':
      ids = [message['roadEncodeIdx']['frameId'] for message in logs[1]['rlog'] if 'roadEncodeIdx' in message]
      assert ids == [*range(1000, 1201), 2000, 2002]
    if scenario == 'fairness':
      services = [next(key for key in ('can', 'carState', 'deviceState') if key in message)
                  for message in logs[0]['rlog'] if any(key in message for key in ('can', 'carState', 'deviceState'))]
      assert services == ['can'] * 200 + ['carState'] * 3 + ['deviceState'] + ['can'] * 300
    return peer, logs
  except BaseException:
    peer.cleanup()
    raise

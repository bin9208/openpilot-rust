"""Independent full-schema and decoded-media assertions for native route outputs."""
from __future__ import annotations

from collections import Counter
from copy import deepcopy
from hashlib import sha256
import json
from pathlib import Path
import subprocess

from openpilot.cereal.services import SERVICE_LIST
from loggerd_peer import Peer


def normalize(records: list[dict]) -> list[dict]:
  result = []
  for source in records:
    message = deepcopy(source)
    if message.get('logMessage') == 'logger-qa-barrier':
      continue
    if 'initData' in message:
      message['logMonoTime'] = 0
      init = message['initData']
      init['wallTimeNanos'] = 0
      entries = init['commands']['entries']
      provenance = ('loggerd implementation', 'loggerd source commit', 'loggerd source tree')
      init['commands']['entries'] = [entry for entry in entries if entry['key'] not in provenance]
      for entry in init['commands']['entries']:
        if entry['key'] == 'df -h':
          entry['value'] = b'<ambient filesystem usage>'
    if 'sentinel' in message:
      message['logMonoTime'] = 0
    result.append(message)
  return result


def metadata(records: list[dict], rust: bool) -> None:
  init = records[0]['initData']
  assert init['dongleId'] == 'host-qa-fixture'
  assert init['gitCommit'] == '1' * 40
  assert init['gitBranch'] == 'host-qa'
  params = {entry['key']: entry.get('value', b'') for entry in init['params']['entries']}
  assert params['AccessToken'] == b''
  assert params['RouteCount'] == b'43'
  assert params['DongleId'] == b'host-qa-fixture'
  commands = {entry['key']: entry['value'] for entry in init['commands']['entries']}
  if rust:
    assert commands['loggerd implementation'] == b'openpilot-loggerd Rust'
    assert len(commands['loggerd source commit']) == 40
    assert int(commands['loggerd source commit'], 16) > 0
    assert commands['loggerd source tree'] in (b'clean', b'dirty', b'unknown')


def decimation(peer: Peer, logs: list[dict]) -> dict:
  expected_rlog = Counter()
  expected_qlog = Counter()
  counters = Counter()
  for service, raw in peer.inputs:
    if service.endswith('EncodeData'):
      continue
    expected_rlog[raw] += 1
    frequency = SERVICE_LIST[service].decimation
    if frequency is not None and counters[service] % frequency == 0:
      expected_qlog[raw] += 1
    counters[service] += 1
  for name, expected in [('rlog', expected_rlog), ('qlog', expected_qlog)]:
    content = b''.join((peer.segment(index) / (name + '.capnp')).read_bytes() for index in range(len(logs)))
    for raw, count in expected.items():
      assert content.count(raw) == count, (name, count, content.count(raw), len(raw))
    if name == 'qlog':
      for raw in expected_rlog.keys() - expected_qlog.keys():
        assert content.count(raw) == 0, 'unexpected qlog payload'
  return {'rlog_ordinary_packets': sum(expected_rlog.values()), 'qlog_ordinary_packets': sum(expected_qlog.values()),
          'ordinary_services': dict(counters)}


def media(path: Path) -> dict:
  probe = subprocess.run(['ffprobe', '-v', 'error', '-show_streams', '-show_packets', '-of', 'json', str(path)],
                         capture_output=True, check=True)
  (path.parent / (path.name + '.probe.json')).write_bytes(probe.stdout)
  decoded = json.loads(probe.stdout)
  fields = ('codec_name', 'codec_type', 'width', 'height', 'time_base', 'sample_rate', 'channels')
  streams = [{field: stream[field] for field in fields if field in stream} for stream in decoded['streams']]
  packet_fields = ('stream_index', 'pts', 'dts', 'duration', 'size', 'flags')
  packets = [{field: packet[field] for field in packet_fields if field in packet} for packet in decoded['packets']]
  video = subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-i', str(path), '-map', '0:v:0', '-fps_mode', 'passthrough', '-f', 'framemd5', '-'],
                         capture_output=True, check=True)
  (path.parent / (path.name + '.frames.md5')).write_bytes(video.stdout)
  frames = [line.strip() for line in video.stdout.decode().splitlines() if line and not line.startswith('#')]
  assert frames, path
  result = {'streams': streams, 'packets': packets, 'frames': frames}
  if any(stream['codec_type'] == 'audio' for stream in streams):
    audio = subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-i', str(path), '-map', '0:a:0', '-f', 'f32le', '-'],
                           capture_output=True, check=True)
    (path.parent / (path.name + '.audio.f32')).write_bytes(audio.stdout)
    assert audio.stdout
    result['audio_bytes'] = len(audio.stdout)
    result['audio_sha256'] = sha256(audio.stdout).hexdigest()
  return result


def compare(source: tuple[Peer, list[dict]], candidate: tuple[Peer, list[dict]]) -> dict:
  original_peer, original = source
  rust_peer, rust = candidate
  assert len(original) == len(rust)
  evidence = {'segments': len(rust), 'original': decimation(original_peer, original), 'rust': decimation(rust_peer, rust), 'media': []}
  for index, (original_segment, rust_segment) in enumerate(zip(original, rust, strict=True)):
    for name in ('rlog', 'qlog'):
      metadata(original_segment[name], False)
      metadata(rust_segment[name], True)
      actual, expected = normalize(rust_segment[name]), normalize(original_segment[name])
      mismatch = [(i, a, b) for i, (a, b) in enumerate(zip(actual, expected, strict=False)) if a != b][:1]
      assert actual == expected, (index, name, mismatch, len(actual), len(expected))
    original_files = [name for name in original_segment['files'] if name.endswith(('.hevc', '.ts'))]
    rust_files = [name for name in rust_segment['files'] if name.endswith(('.hevc', '.ts'))]
    assert original_files == rust_files
    for name in original_files:
      original_media = media(original_peer.segment(index) / name)
      rust_media = media(rust_peer.segment(index) / name)
      assert original_media == rust_media, (index, name, original_media, rust_media)
      evidence['media'].append({'segment': index, 'file': name, 'frames': len(rust_media['frames']),
                                 'streams': rust_media['streams'], 'audio_bytes': rust_media.get('audio_bytes', 0)})
  return evidence

#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_live.py --binary PATH --output NEW_DIR
# Caller provides original dependencies and ORIGINAL_PARAMS_BINDING; owned loopback and synthetic IPC only.
from __future__ import annotations
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import uuid

import anyio
from aiohttp import ClientSession
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save
from carrot_server_live_cases import Frame, event
from carrot_server_live_wire import body, binary_packet, codec, fetch, normalize


def wait_camera_readers(peers: list[Peer], namespace: str) -> None:
  deadline = time.monotonic() + 3
  while time.monotonic() < deadline:
    if all(f'/dev/shm/msgq_{namespace}/livestreamRoadEncodeData' in Path(f'/proc/{peer.process.pid}/maps').read_text() for peer in peers):
      return
    time.sleep(0.005)
  raise TimeoutError('owned camera subscriptions were not observed')


async def scenario(binary: Path, output: Path, selected: list[Frame], unavailable: bool = False) -> None:
  await anyio.Path(output).mkdir()
  namespace = 'rust-probe-live-' + uuid.uuid4().hex
  await anyio.Path('/dev/shm', 'msgq_' + namespace).mkdir()
  params = output / 'params' / namespace
  params.mkdir(parents=True)
  (params / 'IsMetric').write_bytes(b'1')
  (params / 'ShowPathMode').write_bytes(b'owned\xffvalue')
  env = {**os.environ, 'OPENPILOT_PREFIX': namespace, 'PARAMS_ROOT': str(output / 'params'), 'CARROT_DATA_DIR': str(output / 'data')}
  peers = [Peer(output / name) for name in ('source', 'native', 'publisher')]
  for peer in peers:
    peer.output.mkdir()
  observations = []
  packets = []
  publisher = peers[2]
  try:
    names = [row['service'] for row in selected] + ['navInstructionCarrot', 'navRoute', 'roadEncodeData', 'livestreamRoadEncodeData']
    await startup(
      publisher, publisher.start([sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_publisher.py'))], {'services': names}, env, True)
    )
    for peer, command in zip(peers[:2], ([sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_source.py'))], [str(binary)]), strict=True):
      await startup(
        peer, peer.start(command, {'params': True, 'unavailable': unavailable, 'output': str(peer.output), 'state': str(output / 'state')}, env, True)
      )
    async with ClientSession() as client:
      if unavailable:
        paths = ['/api/live_runtime', '/ws/raw/carState', '/ws/raw_multiplex?services=carState', '/ws/compact_state?services=carState', '/ws/camera/road']
        for path in paths:
          responses = [await fetch(peer, path) for peer in peers[:2]]
          assert [(row['status'], base64.b64decode(row['body_base64'])) for row in responses] == [
            (responses[0]['status'], base64.b64decode(responses[0]['body_base64']))
          ] * 2
          assert responses[0]['status'] == 503
          observations.append({'path': path, 'responses': responses})
      else:
        paths = [
          '/ws/raw/nope',
          '/ws/raw_multiplex',
          '/ws/raw_multiplex?services=nope,nope',
          '/ws/compact_state',
          '/ws/compact_state?services=nope',
          '/ws/camera/wide',
          '/ws/raw/carState',
        ]
        for path in paths:
          responses = [await fetch(peer, path) for peer in peers[:2]]
          assert [(row['status'], base64.b64decode(row['body_base64'])) for row in responses] == [
            (responses[0]['status'], base64.b64decode(responses[0]['body_base64']))
          ] * 2
          observations.append({'path': path, 'responses': responses})
        raw = []
        multiplex = []
        compact = []
        camera = []
        for path, destination in [
          ('/ws/raw/carState', raw),
          ('/ws/raw_multiplex?services=carState,carState', multiplex),
          ('/ws/compact_state?services=carState,carState', compact),
          ('/ws/camera/road', camera),
        ]:
          for peer in peers[:2]:
            destination.append(await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}' + path, compress=0))
          hello = [await ws.receive_json() for ws in destination]
          assert hello[0] == hello[1]
          observations.append({'hello_path': path, 'hello': hello})
          if path == '/ws/raw/carState':
            first = next(row for row in selected if row['service'] == 'carState')
            await publisher.control({'frames': [first]})
            first_packets = []
            for ws in destination:
              first_packets.append(await binary_packet(ws))
              save(
                output / 'WS-first-observation.json',
                {
                  'received_providers': list(('source', 'native')[: len(first_packets)]),
                  'packets': [base64.b64encode(packet).decode() for packet in first_packets],
                },
              )
            assert first_packets[0] == first_packets[1] == await anyio.Path(first['path']).read_bytes()
            packets.append({'mode': 'WS-first-no-HTTP', 'bytes': base64.b64encode(first_packets[0]).decode(), 'equal': True})
        row = next(row for row in selected if row['service'] == 'carState')
        await publisher.control({'frames': [row]})
        for mode, connections in [('single', raw), ('multiplex', multiplex), ('compact', compact)]:
          wire = [await binary_packet(ws) for ws in connections]
          assert wire[0] == wire[1]
          if mode == 'single':
            assert wire[0] == await anyio.Path(row['path']).read_bytes()
          if mode == 'multiplex':
            assert wire[0] == b'\x08carState' + await anyio.Path(row['path']).read_bytes()
          packets.append({'mode': mode, 'bytes': base64.b64encode(wire[0]).decode(), 'equal': True})
        nav = event(
          output, 'navInstructionCarrot', {'maneuverPrimaryText': 'owned populated navigation', 'maneuverDistance': 35.5, 'maneuverType': 'turn'}, 'nav'
        )
        route = event(
          output,
          'navRoute',
          {'coordinates': [{'latitude': 37.5, 'longitude': 127.0}, {'latitude': 0.0, 'longitude': 0.0}, {'latitude': 91.0, 'longitude': 1.0}]},
          'route',
        )
        enabled = event(output, 'selfdriveState', {'enabled': True, 'alertText1': 'owned'}, 'engaged')
        await publisher.control({'frames': [nav, route, enabled]})
        responses = [await fetch(peer, '/api/live_runtime') for peer in peers[:2]]
        assert all(row['status'] == 200 for row in responses)
        assert normalize(body(responses[0])) == normalize(body(responses[1]))
        observations.append({'path': '/api/live_runtime', 'responses': responses})
        assert body(responses[0])['services']['navInstructionCarrot'] == {'mainText': None, 'distanceText': None, 'turnType': None}
        for peer in peers[:2]:
          cached = await fetch(peer, '/api/live_runtime?force=1')
          assert body(cached)['meta']['generatedAtMs'] == body(responses[peers.index(peer)])['meta']['generatedAtMs']
        (params / 'IsMetric').chmod(0)
        try:
          await anyio.sleep(0.13)
          await publisher.control({'frames': [nav, route, enabled]})
          permission = [await fetch(peer, '/api/live_runtime?force=1') for peer in peers[:2]]
          save(output / 'params-error-observation.json', {'responses': permission})
          assert normalize(body(permission[0])) == normalize(body(permission[1]))
          assert body(permission[0])['runtime']['params']['IsMetric'] == '0'
        finally:
          (params / 'IsMetric').chmod(0o600)
        head = [await fetch(peer, '/api/live_runtime', 'HEAD') for peer in peers[:2]]
        assert [row['status'] for row in head] == [200, 200]
        observations.append({'head': head})
        state = event(output, 'roadCameraState', {'frameId': 71}, 'ready-camera')
        video = event(
          output,
          'livestreamRoadEncodeData',
          {
            'idx': {'frameId': 73, 'type': 'livestreamH264', 'flags': 8, 'encodeId': 9, 'segmentId': 2},
            'width': 640,
            'height': 360,
            'header': b'\x00\x00\x00\x01\x67\x42\x80\x1f',
            'data': b'\x00\x00\x01\x65owned-h264-payload',
          },
          'camera',
        )
        await publisher.control({'frames': [state]})
        await anyio.to_thread.run_sync(wait_camera_readers, peers[:2], namespace)
        save(output / 'camera-readiness.json', {'owned_pids': [peer.process.pid for peer in peers[:2]], 'candidate_subscriptions_observed': True})
        await publisher.control({'frames': [video]})
        images = [await binary_packet(ws) for ws in camera]
        decoded = []
        for packet in images:
          size = int.from_bytes(packet[:4], 'big')
          decoded.append({'meta': normalize(json.loads(packet[4 : 4 + size])), 'video': base64.b64encode(packet[4 + size :]).decode()})
        assert decoded[0] == decoded[1]
        packets.append({'mode': 'camera', 'packets': decoded, 'equal': True})
        for connections in (raw, multiplex, compact, camera):
          for ws in connections:
            await ws.close()
        await anyio.sleep(5.2)
        idle = []
        for provider, peer in zip(('source', 'native'), peers[:2], strict=True):
          maps = await anyio.Path(f'/proc/{peer.process.pid}/maps').read_text()
          present = [
            name for name in ('carState', 'roadCameraState', 'livestreamRoadEncodeData', 'roadEncodeData') if f'/dev/shm/msgq_{namespace}/{name}' in maps
          ]
          idle.append({'provider': provider, 'mapped_hub_services': present})
        save(output / 'idle-observation.json', idle)
        assert all(not row['mapped_hub_services'] for row in idle)
        observations.append({'lazy_idle': idle})
        restarted = [await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}/ws/raw/carState', compress=0) for peer in peers[:2]]
        for ws in restarted:
          await ws.receive_json()
        await publisher.control({'frames': [row]})
        restarted_packets = [await binary_packet(ws) for ws in restarted]
        assert restarted_packets[0] == restarted_packets[1] == await anyio.Path(row['path']).read_bytes()
        packets.append({'mode': 'WS-first-after-idle', 'equal': True})
        for ws in restarted:
          await ws.close()
  finally:
    failed = sys.exc_info()[0] is not None
    save(output / 'observations.json', observations)
    save(output / 'packets.json', packets)
    errors = []
    for peer in peers:
      try:
        await peer.close()
      except (AssertionError, OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
        errors.append(f'{type(error).__name__}: {error}')
    save(output / 'cleanup.json', {'errors': errors})
    import shutil

    shutil.rmtree(Path('/dev/shm') / ('msgq_' + namespace), ignore_errors=True)
    if not failed:
      assert not errors


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--reuse-codec', type=Path)
  args = parser.parse_args()
  binary = args.binary.resolve()
  output = args.output.resolve()
  output.mkdir(parents=True)
  save(
    output / 'invocation.json',
    {
      'argv': [sys.executable, '-P', *sys.argv],
      'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
      'normalization': 'generatedAtMs/snapshotAgeMs/camera ts only; semantic fields and payload bytes unmasked',
    },
  )
  if args.reuse_codec:
    reference = args.reuse_codec.resolve()
    prior = json.loads(reference.read_text())
    assert prior['equal']
    selected = prior['input']
    save(
      output / 'codec-reuse.json',
      {'reference': str(reference), 'comparisons': prior['comparisons'], 'scope': 'unchanged compact encoder/schema/value bodies; owner/broker fixes only'},
    )
  else:
    selected = await codec(binary, output)
  await scenario(binary, output / 'family', selected)
  await scenario(binary, output / 'unavailable', selected, True)
  save(
    output / 'result.json',
    {
      'compact_comparisons': len(selected),
      'whole_family_equal': True,
      'unavailable_pairs': 5,
      'cleanup': 'all owned peers exited0; lazy IPC readers released after5s',
    },
  )
  print(json.dumps({'status': 'PASS', 'compact_comparisons': len(selected)}))


if __name__ == '__main__':
  anyio.run(main)

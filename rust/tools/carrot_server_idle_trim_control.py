#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import time
import uuid

import anyio
from aiohttp import ClientSession
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save


def calls(peer: Peer) -> list[dict]:
  path = peer.output / 'trim-calls.jsonl'
  return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


async def run(args: argparse.Namespace) -> None:
  output = args.output.resolve()
  output.mkdir(parents=True)
  namespace = 'rust-probe-trim-' + uuid.uuid4().hex
  queue = Path('/dev/shm') / ('msgq_' + namespace)
  queue.mkdir()
  params = output / 'params' / namespace
  params.mkdir(parents=True)
  (params / 'CarrotVisionActive').write_bytes(b'0')
  env = {**os.environ, 'OPENPILOT_PREFIX': namespace, 'PARAMS_ROOT': str(output / 'params'), 'CARROT_DATA_DIR': str(output / 'data')}
  peers = [Peer(output / name) for name in ('source', 'native')]
  observations = []
  began = time.monotonic()
  try:
    for peer in peers:
      peer.output.mkdir()
      source = peer is peers[0]
      command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_source.py'))] if source else [str(args.binary.resolve())]
      child_env = env if source else {**env, 'LD_PRELOAD': str(args.preload.resolve()), 'OWNED_TRIM_RECEIPT': str(peer.output / 'trim-calls.jsonl')}
      await startup(peer, peer.start(command, {'params': True, 'unavailable': False, 'composed': True, 'idle_trim': True, 'output': str(peer.output), 'state': str(output / 'state')}, child_env, True))
      save(peer.output / 'identity.json', {'exe_sha256': hashlib.sha256(Path(f'/proc/{peer.process.pid}/exe').read_bytes()).hexdigest(), 'preload_sha256': hashlib.sha256(args.preload.read_bytes()).hexdigest() if not source else None})
    async with ClientSession() as client:
      camera = [await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}/ws/camera/road') for peer in peers]
      camera_hello = [await ws.receive_json() for ws in camera]
      assert camera_hello[0] == camera_hello[1]
      if args.red:
        await anyio.sleep(32)
        observed = [calls(peer) for peer in peers]
        save(output / 'red.json', {'source_calls': observed[0], 'native_calls': observed[1], 'camera_only': True})
        assert len(observed[0]) == 1 and len(observed[1]) == 0
      else:
        raw = [await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}/ws/raw/carState') for peer in peers]
        raw_hello = [await ws.receive_json() for ws in raw]
        assert raw_hello[0] == raw_hello[1]
        await anyio.sleep(32)
        observations.append({'phase': 'raw_connected', 'seconds': time.monotonic() - began, 'calls': [calls(peer) for peer in peers]})
        assert all(not calls(peer) for peer in peers)
        for ws in raw:
          await ws.close()
        (params / 'CarrotVisionActive').write_bytes(b'1')
        await anyio.sleep(30)
        observations.append({'phase': 'vision_active', 'seconds': time.monotonic() - began, 'calls': [calls(peer) for peer in peers]})
        assert all(not calls(peer) for peer in peers)
        (params / 'CarrotVisionActive').write_bytes(b'0')
        await anyio.sleep(30)
        observed = [calls(peer) for peer in peers]
        observations.append({'phase': 'camera_only', 'seconds': time.monotonic() - began, 'calls': observed})
        save(output / 'observations.json', observations)
        assert all(len(rows) == 1 for rows in observed)
        assert all(89 <= rows[0]['time'] - began <= 92 for rows in observed)
      for ws in camera:
        await ws.close()
  finally:
    save(output / 'observations.json', observations)
    with anyio.CancelScope(shield=True):
      for peer in peers:
        await peer.close()
      shutil.rmtree(queue)
  save(output / 'result.json', {'red_missing_native_loop': args.red, 'guard_pair': not args.red, 'camera_only_trim_pair': True, 'owned_process_exit': [peer.process.returncode for peer in peers]})
  print('PASS')


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'preload', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--red', action='store_true')
  args = parser.parse_args()
  await run(args)


if __name__ == '__main__':
  anyio.run(main)

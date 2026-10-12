#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned missing-symbol provider: actual source/native diagnostics and App isolation."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess

import anyio
from carrot_server_dashcam_upload import request, save
from carrot_server_youtube_fixture import Fixture, Peer, Provider
from carrot_server_youtube_probe import Probe


async def run(args: argparse.Namespace) -> None:
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  free = await anyio.to_thread.run_sync(lambda: shutil.disk_usage(output).free)
  minimum = 25 * 2**30 + 3648 * 2**20
  assert free >= minimum
  library = output / 'librtmp.so.1'
  argv = ['cc', '-x', 'c', '-shared', '-fPIC', '-o', str(library), '-']
  result = await anyio.to_thread.run_sync(lambda: subprocess.run(argv, input=b'int owned_placeholder(void) { return 1; }\n', capture_output=True, timeout=10))
  await anyio.to_thread.run_sync(save, output / 'provider-build.json', {'argv': argv, 'exit': result.returncode, 'free': free, 'minimum': minimum})
  assert result.returncode == 0
  os.environ['LD_LIBRARY_PATH'] = str(output)
  os.environ['ORIGINAL_PARAMS_BINDING'] = str(await anyio.Path(args.binding).resolve())
  binary = Path(str(await anyio.Path(args.binary).resolve()))
  certs = (args.certificate, args.key)
  fixture = Fixture(output, binary, certs)
  for peer in fixture.peers:
    await anyio.to_thread.run_sync(peer.probe.close)
  fixture.peers = [Peer(Provider(name, binary, True), output / name, Probe(*certs)) for name in ['source', 'native']]
  diagnostics = []
  try:
    await fixture.start()
    for peer in fixture.peers:
      response = await request(peer.port, '/api/youtube_live/diagnostics')
      await anyio.to_thread.run_sync(save, peer.root / 'diagnostics.json', response)
      assert response['status'] == 200
      diagnostics.append(response['payload'])
      await anyio.to_thread.run_sync(save, peer.root / 'key.json', await request(peer.port, '/api/youtube_live/stream_key'))
    native = await request(fixture.peers[1].port, '/api/heartbeat_status')
    await anyio.to_thread.run_sync(save, output / 'app-isolation.json', native)
    assert native['status'] == 200
  finally:
    with anyio.CancelScope(shield=True):
      await fixture.close()
  transports = [value['diagnostics']['transport'] for value in diagnostics]
  equal = (
    all(not value['available'] for value in transports)
    and transports[0]['missing_symbols'] == transports[1]['missing_symbols']
    and transports[0]['error'] == transports[1]['error']
  )
  await anyio.to_thread.run_sync(save, output / 'result.json', {'transports': transports, 'native_app_survives': True, 'pass': equal})
  assert equal, transports


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'binding', 'certificate', 'key', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

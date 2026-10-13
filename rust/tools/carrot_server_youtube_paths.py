#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Raw request targets at the original-router/native-Application decoding boundary."""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import anyio
from carrot_server_dashcam_upload import request, save
from carrot_server_youtube_fixture import Fixture, Peer, Provider
from carrot_server_youtube_probe import Probe


async def run(args: argparse.Namespace) -> None:
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  binary = Path(str(await anyio.Path(args.binary).resolve()))
  certs = (Path(str(await anyio.Path(args.certificate).resolve())), Path(str(await anyio.Path(args.key).resolve())))
  os.environ['ORIGINAL_PARAMS_BINDING'] = str(await anyio.Path(args.binding).resolve())
  fixture = Fixture(output, binary, certs)
  for peer in fixture.peers:
    await anyio.to_thread.run_sync(peer.probe.close)
  fixture.peers = [Peer(Provider(name, binary, True), output / name, Probe(*certs)) for name in ['source', 'native']]
  rows = []
  try:
    await fixture.start()
    for name, suffix in [('plain', 'status'), ('single-escaped', '%73tatus'), ('double-escaped', '%2573tatus')]:
      for method in ['GET', 'POST']:
        path = '/api/youtube_live/' + suffix
        captured = [await request(peer.port, path, method) for peer in fixture.peers]
        for peer, response in zip(fixture.peers, captured, strict=True):
          await anyio.to_thread.run_sync(save, output / f'{name}-{method}-{peer.name}.json', response)
        expected = 404 if name == 'double-escaped' and method == 'GET' else 200 if method == 'GET' else 405
        statuses = [response['status'] for response in captured]
        rows.append({'raw_path': path, 'method': method, 'expected': expected, 'statuses': statuses, 'equal': statuses == [expected, expected]})
  finally:
    with anyio.CancelScope(shield=True):
      await fixture.close()
  await anyio.to_thread.run_sync(save, output / 'result.json', {'pairs': rows, 'pass': all(row['equal'] for row in rows)})
  assert all(row['equal'] for row in rows), rows


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'binding', 'certificate', 'key', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

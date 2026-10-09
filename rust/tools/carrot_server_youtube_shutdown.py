#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Held original/native health requests drain through actual App cleanup, including disconnected callers."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import threading
import time

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_upload import save
from carrot_server_youtube_fixture import Peer, Provider
from carrot_server_youtube_probe import Probe


class HeldProbe(Probe):
  def __init__(self, certificate: Path, key: Path) -> None:
    self.accepted = threading.Event()
    self.eof = False
    self.duration = 0.0
    super().__init__(certificate, key)

  def run(self) -> None:
    began = time.monotonic()
    try:
      while not self.stop.is_set():
        try:
          raw, _address = self.listener.accept()
          break
        except TimeoutError:
          continue
      else:
        return
      self.sockets.append(raw)
      raw.settimeout(5)
      hello = raw.recv(65536)
      assert hello and hello[0] == 22, 'health call must send a real TLS ClientHello'
      self.accepted.set()
      while raw.recv(65536):
        continue
      self.eof = True
    except OSError as error:
      self.errors.append(str(error))
    finally:
      self.duration = time.monotonic() - began
      self.observations.append({'tls': False, 'error': 'owned recipient held handshake'})
      for raw in self.sockets:
        raw.close()


async def observe(peer: Peer, probe: HeldProbe, disconnect: bool) -> None:
  stream = await anyio.connect_tcp('127.0.0.1', peer.port)
  process = peer.process
  assert process and process.stdin
  body = b'{"stream_key":"owned-stream-key"}'
  request = (
    'POST /api/youtube_live/stream_key/validate HTTP/1.1\r\n'
    + 'Host: localhost\r\nConnection: close\r\nContent-Type: application/json\r\n'
    + f'Content-Length: {len(body)}\r\n\r\n'
  ).encode() + body
  try:
    await stream.send(request)
    assert await anyio.to_thread.run_sync(probe.accepted.wait, 3)
    if disconnect:
      await stream.aclose()
    began = time.monotonic()
    await process.stdin.send(b'\n')
    await process.stdin.aclose()
    with anyio.fail_after(7):
      await process.wait()
    seconds = time.monotonic() - began
    assert process.returncode == 0 and 1.5 <= seconds < 5, seconds
    response = None
    if not disconnect:
      with anyio.fail_after(1):
        response = await read_response(BufferedByteReceiveStream(stream), 'POST')
      assert response['status'] == 409
    await anyio.to_thread.run_sync(probe.thread.join, 1)
    assert not probe.thread.is_alive() and probe.eof
    await anyio.to_thread.run_sync(
      save,
      peer.root / 'observation.json',
      {
        'client_disconnected': disconnect,
        'real_tls_client_hello': True,
        'stop_seconds': seconds,
        'recipient_eof': probe.eof,
        'recipient_thread_exited': True,
        'process_pid': process.pid,
        'process_exit': process.returncode,
        'process_reaped': True,
        'response': response,
        'same_app_grace_window': True,
      },
    )
  finally:
    await stream.aclose()


async def run(args: argparse.Namespace) -> None:
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  binary = Path(str(await anyio.Path(args.binary).resolve()))
  certificate = Path(str(await anyio.Path(args.certificate).resolve()))
  key = Path(str(await anyio.Path(args.key).resolve()))
  os.environ['ORIGINAL_PARAMS_BINDING'] = str(await anyio.Path(args.binding).resolve())
  rows = []
  for disconnect in [False, True]:
    for name in ['source', 'native']:
      root = output / (name + ('-disconnected' if disconnect else '-connected'))
      probe = HeldProbe(certificate, key)
      peer = Peer(Provider(name, binary, True), root, probe)
      try:
        await peer.start(certificate)
        await observe(peer, probe, disconnect)
        rows.append({'provider': name, 'disconnected': disconnect, 'pass': True})
      finally:
        with anyio.CancelScope(shield=True):
          assert not await peer.close()
  await anyio.to_thread.run_sync(save, output / 'result.json', {'cases': rows, 'actual_app_cleanup': True, 'held_probe_drain_after_disconnect': True})


def main() -> None:
  parser = argparse.ArgumentParser()
  for field in ['binary', 'binding', 'certificate', 'key', 'output']:
    parser.add_argument('--' + field, type=Path, required=True)
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

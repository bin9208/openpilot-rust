#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import base64
import json
from pathlib import Path
import sys
import time
from enum import StrEnum
from typing import TypedDict, assert_never

import anyio
from aiohttp import ClientSession, WSMsgType
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Inputs, Json
from carrot_server_terminal_http_fixture import Peer
from carrot_server_terminal_pty import output


class Mode(StrEnum):
  HISTORY = 'history'
  SHUTDOWN = 'shutdown'
  MISSING = 'missing'
  PARAMS = 'params'


class HistoryObservation(TypedDict):
  pid: int
  captured_bytes: int
  history_bytes: int
  replay_bytes: int
  promoted: dict[str, Json]
  resize: dict[str, Json]
  raw_clear_ctrl_c_usable: bool


class ShutdownObservation(TypedDict):
  client_close: int | None
  cleanup: dict[str, Json]
  seconds: float
  held_until_client_close: bool
  pid: int


class MissingObservation(TypedDict):
  error: str
  close: int
  status: dict[str, Json]


async def history(peer: Peer) -> HistoryObservation:
  async with ClientSession() as client:
    url = f'http://127.0.0.1:{peer.port}'
    async with client.ws_connect(url + '/ws/terminal_pty') as first:
      meta = await first.receive_json()
      await first.send_json({'type': 'input', 'data': "head -c 540000 /dev/zero | tr '\\0' x; printf '\\nOWNED_CAP\\n'"})
      captured = await output(first, b'OWNED_CAP\r\n')
      async with client.get(url + '/api/terminal_pty/status') as response:
        state = await response.json()
      assert state['history_bytes'] == 512 * 1024
      async with client.ws_connect(url + '/ws/terminal_pty') as second:
        secondary = await second.receive_json()
        replay = await second.receive_json()
        replay_bytes = base64.b64decode(replay['b64'])
        assert not secondary['primary'] and replay['replay'] and len(replay_bytes) == 512 * 1024
        await first.close()
        async with client.get(url + '/api/terminal_pty/status') as response:
          promoted = await response.json()
        assert promoted['clients'] == 1 and promoted['primary']
        await second.send_json({'type': 'resize', 'rows': 1000, 'cols': 1})
        resized = await second.receive_json()
        assert resized['type'] == 'pty_resize' and resized['rows'] == 200 and resized['cols'] == 100
        await second.send_json({'type': 'control', 'action': 'clear'})
        await second.send_json({'type': 'raw', 'data': "printf 'OWNED_RAW\\n'\r"})
        await output(second, b'OWNED_RAW\r\n')
        await second.send_json({'type': 'input', 'data': 'sleep 10'})
        await output(second, b'sleep 10')
        await second.send_json({'type': 'control', 'action': 'ctrl_c'})
        await second.send_json({'type': 'input', 'data': "printf 'OWNED_INTERRUPT\\n'"})
        await output(second, b'OWNED_INTERRUPT\r\n')
    return {
      'pid': meta['pid'],
      'captured_bytes': len(captured),
      'history_bytes': state['history_bytes'],
      'replay_bytes': len(replay_bytes),
      'promoted': promoted,
      'resize': resized,
      'raw_clear_ctrl_c_usable': True,
    }


async def shutdown(peer: Peer) -> ShutdownObservation:
  assert peer.process and peer.process.stdin and peer.reader
  async with ClientSession() as client:
    async with client.ws_connect(f'http://127.0.0.1:{peer.port}/ws/terminal_pty') as socket:
      meta = await socket.receive_json()
      await socket.send_json({'type': 'input', 'data': "printf 'OWNED_HELD\\n'"})
      await output(socket, b'OWNED_HELD\r\n')
      began = time.monotonic()
      await peer.process.stdin.send(b'cleanup\n')
      with anyio.move_on_after(0.2) as wait:
        await peer.reader.receive_until(b'\n', 65536)
      assert wait.cancel_called, 'cleanup discarded a held terminal session'
      await socket.close()
      with anyio.fail_after(4):
        cleanup = json.loads(await peer.reader.receive_until(b'\n', 65536))
      if peer.source:
        assert cleanup['pty']['alive']
      else:
        assert cleanup['terminal_reaped']
        assert not await anyio.Path(f'/proc/{meta["pid"]}').exists()
      return {'client_close': socket.close_code, 'cleanup': cleanup, 'seconds': time.monotonic() - began, 'held_until_client_close': True, 'pid': meta['pid']}


async def missing(peer: Peer) -> MissingObservation:
  async with ClientSession() as client:
    async with client.ws_connect(f'http://127.0.0.1:{peer.port}/ws/terminal_pty') as socket:
      error = await socket.receive_json()
      packet = await socket.receive()
      assert error['type'] == 'error' and 'No such file or directory' in error['error']
      assert packet.type == WSMsgType.CLOSE and packet.data == 1000
      async with client.get(f'http://127.0.0.1:{peer.port}/api/terminal_pty/status') as response:
        state = await response.json()
      assert not state['alive'] and state['clients'] == 0
      return {'error': error['error'].replace(str(peer.root), '$OWNED'), 'close': packet.data, 'status': state}


async def params_failure(peer: Peer) -> dict[str, Json]:
  root = anyio.Path(peer.fixture.params)
  saved = anyio.Path(peer.root / 'saved-params')
  await root.rename(saved)
  await root.write_text('owned constructor failure')
  try:
    async with ClientSession() as client:
      async with client.get(f'http://127.0.0.1:{peer.port}/api/vision_test/status') as response:
        data = await response.json()
        observed = {'status': response.status, 'device': data.get('device'), 'ok': data.get('ok')}
      async with client.get(f'http://127.0.0.1:{peer.port}/api/heartbeat_status') as response:
        assert response.status == 200
        observed['sibling_alive'] = True
    return observed
  finally:
    await root.unlink()
    await saved.rename(root)


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  inputs = Inputs(args.native.resolve(), args.launcher.resolve(), args.binding.resolve(), args.vision_root.resolve())
  peers = [Peer(root / name, name == 'source', inputs, args.server.resolve(), args.mode in ['shutdown', 'params']) for name in ['source', 'native']]
  errors = []
  mode = Mode(args.mode)
  try:
    for peer in peers:
      if args.mode == 'missing':
        peer.shell = peer.root / 'missing-shell'
      await peer.start()
    observations = []
    for peer in peers:
      match mode:
        case Mode.HISTORY:
          observation = await history(peer)
        case Mode.SHUTDOWN:
          observation = await shutdown(peer)
        case Mode.MISSING:
          observation = await missing(peer)
        case Mode.PARAMS:
          observation = await params_failure(peer)
        case _:
          assert_never(mode)
      observations.append(observation)
      await anyio.to_thread.run_sync(save, root / 'observations.json', observations)
    if args.mode in ['missing', 'params']:
      assert observations[0] == observations[1]
    await anyio.to_thread.run_sync(save, root / 'result.json', {'mode': args.mode, 'source': observations[0], 'native': observations[1]})
  finally:
    original = sys.exception()
    with anyio.CancelScope(shield=True):
      for peer in peers:
        try:
          await peer.close()
        except (OSError, TimeoutError, ExceptionGroup) as error:
          errors.append(error)
      await anyio.to_thread.run_sync(save, root / 'cleanup.json', {'errors': [str(error) for error in errors]})
    if errors:
      if original:
        original.add_note(repr(errors))
      else:
        raise ExceptionGroup('terminal limits cleanup', errors)


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['server', 'native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--mode', choices=[mode.value for mode in Mode], required=True)
  args = parser.parse_args()
  await anyio.Path(args.output.resolve()).mkdir(parents=True)
  await run(args)


if __name__ == '__main__':
  anyio.run(main)

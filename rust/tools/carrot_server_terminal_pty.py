#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Caller provides the original aiohttp environment; no runtime provider is installed.
# python -P rust/tools/carrot_server_terminal_pty.py --output DIR [--native PATH]
# ──────────────────
"""Actual source/native terminal PTY ownership and wire observations."""

from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from aiohttp import ClientSession, ClientWebSocketResponse, WSMsgType
from carrot_server_dashcam_upload import save
from carrot_server_terminal_providers import prepare


async def output(ws: ClientWebSocketResponse, needle: bytes) -> bytes:
  result = bytearray()
  with anyio.fail_after(4):
    while needle not in result:
      message = await ws.receive()
      assert message.type == WSMsgType.TEXT, message
      payload = json.loads(message.data)
      if payload['type'] == 'pty_output':
        result.extend(base64.b64decode(payload['b64']))
  return bytes(result)


async def observe(root: Path, native: Path | None, launcher: Path | None, syscall_filter: Path | None) -> None:
  environment = await anyio.to_thread.run_sync(prepare, root)
  argv = [str(native)] if native else [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_terminal_source.py'))]
  if syscall_filter:
    argv.insert(0, str(syscall_filter))
  config = {'owned_root': str(root), 'launcher': str(launcher) if launcher else ''}
  pid = 0
  async with await anyio.Path(root / 'process.log').open('wb') as log:
    async with await anyio.open_process(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log.wrapped) as process:
      assert process.stdin and process.stdout
      reader = BufferedByteReceiveStream(process.stdout)
      try:
        await process.stdin.send((json.dumps(config) + '\n').encode())
        with anyio.fail_after(8):
          ready = json.loads(await reader.receive_until(b'\n', 65536))
        url = f'http://127.0.0.1:{ready["port"]}'
        captures = []
        async with ClientSession() as client:
          async with client.ws_connect(url + '/ws/terminal_pty?rows=72&cols=240') as first:
            meta = await first.receive_json()
            captures.append({'first_meta': meta})
            pid = meta['pid']
            assert meta['rows'] == 30 and meta['cols'] == 100
            await first.send_json({'type': 'input', 'data': "printf 'OWNED_LINE\\n'"})
            data = await output(first, b'OWNED_LINE\r\n')
            captures.append({'input_output_base64': base64.b64encode(data).decode()})
            await first.send_json({'type': 'resize', 'rows': 7, 'cols': 222})
            with anyio.fail_after(4):
              while True:
                resized = await first.receive_json()
                if resized['type'] == 'pty_resize':
                  break
            assert resized['rows'] == 8 and resized['cols'] == 100
            captures.append({'resize': resized})
            async with client.ws_connect(url + '/ws/terminal_pty') as second:
              second_meta = await second.receive_json()
              replay = await second.receive_json()
              assert not second_meta['created'] and not second_meta['primary']
              assert replay['replay'] and b'OWNED_LINE' in base64.b64decode(replay['b64'])
              captures.append({'second_meta': second_meta, 'replay': replay})
          async with client.get(url + '/api/terminal_pty/status') as response:
            status = await response.json()
          assert status['alive'] and status['clients'] == 0 and not status['primary']
          captures.append({'after_detach': status})
        await process.stdin.send(b'cleanup\n')
        with anyio.fail_after(5):
          cleanup = json.loads(await reader.receive_until(b'\n', 65536))
        assert cleanup['pty']['alive'] and cleanup['pty']['pid'] == pid
        shell = json.loads((await anyio.Path(root / 'shell.jsonl').read_text()).splitlines()[0])
        assert shell['pid'] == shell['sid'] == pid
        assert not shell['controlling_tty'] and shell['tmux'] is None
        assert shell['size'] == [30, 100, 0, 0]
        assert shell['term'] == 'xterm-256color' and shell['colorterm'] == 'truecolor'
        await process.stdin.send(b'exit\n')
        with anyio.move_on_after(2) as wait:
          await process.wait()
        runtime_exit = {'exited_in_2s': not wait.cancel_called, 'returncode': process.returncode}
        await anyio.to_thread.run_sync(
          save,
          root / 'result.json',
          {
            'argv': argv,
            'config': config,
            'captures': captures,
            'shell_boundary': shell,
            'app_cleanup': cleanup,
            'runtime_exit': runtime_exit,
          },
        )
      finally:
        if pid:
          try:
            if os.getsid(pid) == pid:
              os.killpg(pid, signal.SIGHUP)
          except ProcessLookupError:
            pid = 0
        with anyio.move_on_after(3):
          await process.wait()
        if process.returncode is None:
          process.terminate()
          with anyio.move_on_after(3):
            await process.wait()
        if process.returncode is None:
          process.kill()
          await process.wait()
        await anyio.to_thread.run_sync(save, root / 'cleanup.json', {'server_exit': process.returncode, 'owned_group': pid})


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--native', type=Path)
  parser.add_argument('--launcher', type=Path)
  parser.add_argument('--filter', type=Path)
  options = parser.parse_args()
  root = options.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  started = time.monotonic()
  await observe(root, options.native, options.launcher, options.filter)
  await anyio.to_thread.run_sync(
    save,
    root / 'invocation.json',
    {
      'argv': [sys.executable, '-P', *sys.argv],
      'PYTHONPATH': os.environ.get('PYTHONPATH', ''),
      'seconds': time.monotonic() - started,
    },
  )


if __name__ == '__main__':
  anyio.run(main)

# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python -P rust/tools/carrot_server_terminal_edges.py --output DIR --mode reset|eof
# ──────────────────
"""Bounded actual original PTY reset/natural-exit controls with owned process identities."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys

import anyio
from aiohttp import ClientSession, WSMsgType
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import save
from carrot_server_terminal_providers import prepare
from carrot_server_terminal_pty import output


def cleanup_groups(root: Path) -> list[int]:
  removed = []
  records = (root / 'shell.jsonl').read_text().splitlines() if (root / 'shell.jsonl').exists() else []
  for line in records:
    value = json.loads(line)
    pid = value['pid']
    try:
      stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
      if int(stat[19]) == value['starttime'] and os.getsid(pid) == pid:
        os.killpg(pid, signal.SIGHUP)
        removed.append(pid)
    except (FileNotFoundError, ProcessLookupError):
      removed.append(-pid)
  return removed


async def run(root: Path, mode: str, native: Path | None, launcher: Path | None) -> None:
  environment = await anyio.to_thread.run_sync(prepare, root)
  argv = [str(native)] if native else [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_terminal_source.py'))]
  async with await anyio.Path(root / 'process.log').open('wb') as log:
    async with await anyio.open_process(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log.wrapped) as peer:
      assert peer.stdin and peer.stdout
      reader = BufferedByteReceiveStream(peer.stdout)
      try:
        await peer.stdin.send((json.dumps({'owned_root': str(root), 'launcher': str(launcher) if launcher else ''}) + '\n').encode())
        with anyio.fail_after(8):
          ready = json.loads(await reader.receive_until(b'\n', 65536))
        url = f'http://127.0.0.1:{ready["port"]}'
        async with ClientSession() as client:
          async with client.ws_connect(url + '/ws/terminal_pty') as first:
            first_meta = await first.receive_json()
            await first.send_json({'type': 'input', 'data': "printf 'OWNED_READY\\n'"})
            await output(first, b'OWNED_READY\r\n')
            messages = []
            if mode == 'reset':
              async with client.ws_connect(url + '/ws/terminal_pty?reset=1') as second:
                messages.append(await second.receive_json())
                if native:
                  assert not await anyio.Path(f'/proc/{first_meta["pid"]}').exists()
                  await second.send_json({'type': 'input', 'data': "printf 'OWNED_RESET_READY\\n'"})
                  data = await output(second, b'OWNED_RESET_READY\r\n')
                  messages.append({'usable_new_generation': data.decode(errors='replace'), 'old_pid_reaped': True})
                else:
                  with anyio.move_on_after(0.5):
                    while True:
                      packet = await second.receive()
                      messages.append({'type': packet.type.name, 'data': packet.data})
                      if packet.type in {WSMsgType.CLOSED, WSMsgType.CLOSE, WSMsgType.ERROR}:
                        break
            else:
              await first.send_json({'type': 'input', 'data': 'exit 7'})
              with anyio.fail_after(4):
                while True:
                  packet = await first.receive()
                  messages.append({'type': packet.type.name, 'data': packet.data})
                  if packet.type in {WSMsgType.CLOSED, WSMsgType.CLOSE, WSMsgType.ERROR}:
                    break
          async with client.get(url + '/api/terminal_pty/status') as response:
            status = await response.json()
          await anyio.to_thread.run_sync(
            save,
            root / 'result.json',
            {
              'mode': mode,
              'first_meta': first_meta,
              'messages': messages,
              'final_status': status,
            },
          )
        await peer.stdin.send(b'cleanup\n')
        with anyio.fail_after(4):
          cleanup = json.loads(await reader.receive_until(b'\n', 65536))
        await anyio.to_thread.run_sync(save, root / 'app-cleanup.json', cleanup)
        await peer.stdin.send(b'exit\n')
      finally:
        removed = await anyio.to_thread.run_sync(cleanup_groups, root)
        with anyio.move_on_after(3):
          await peer.wait()
        if peer.returncode is None:
          peer.terminate()
          with anyio.move_on_after(3):
            await peer.wait()
        if peer.returncode is None:
          peer.kill()
          await peer.wait()
        await anyio.to_thread.run_sync(
          save,
          root / 'cleanup.json',
          {
            'server_exit': peer.returncode,
            'owned_pid_starttime_groups': removed,
          },
        )


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--mode', choices=['reset', 'eof'], required=True)
  parser.add_argument('--native', type=Path)
  parser.add_argument('--launcher', type=Path)
  options = parser.parse_args()
  root = options.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  await run(root, options.mode, options.native, options.launcher)


if __name__ == '__main__':
  anyio.run(main)

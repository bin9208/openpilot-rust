#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import time
from typing import TypedDict

import anyio
from aiohttp import ClientSession, ClientTimeout
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Inputs, Json
from carrot_server_terminal_http_fixture import Peer


def provider(peer: Peer) -> None:
  path = peer.root / 'bin' / ('python3' if peer.source else 'carrot-command')
  path.write_text(
    '\n'.join(
      [
        f'#!{sys.executable}',
        'import json,os,time',
        'from pathlib import Path',
        f'root=Path({str(peer.root)!r})',
        'stat=Path(f"/proc/{os.getpid()}/stat").read_text().rsplit(")",1)[1].split()',
        '(root/"capture-child.json").write_text(json.dumps({"pid":os.getpid(),"starttime":int(stat[19])}))',
        'deadline=time.monotonic()+55',
        'while not (root/"release").exists() and time.monotonic()<deadline: time.sleep(.02)',
        '(root/"capture-finished").write_text("completed")',
        'os.write(1,b"  owned output \\xff\\n")',
        'os.write(2,b"  owned error \\xe2\\x82\\n")',
      ]
    )
    + '\n'
  )
  path.chmod(0o755)


def wait_path(path: Path, present: bool) -> None:
  deadline = time.monotonic() + 3
  while path.exists() != present:
    if time.monotonic() >= deadline:
      raise TimeoutError(str(path))
    time.sleep(0.02)


class CaptureObservation(TypedDict, total=False):
  status: int
  body: dict[str, Json]
  seconds: float
  continued_after_disconnect: bool
  owned_child: dict[str, int]
  child_reaped: bool
  force_cleanup: dict[str, Json]
  force_wire: str


async def exercise(peer: Peer, mode: str) -> CaptureObservation:
  await anyio.to_thread.run_sync(provider, peer)
  url = f'http://127.0.0.1:{peer.port}/api/terminal_commands/run'
  began = time.monotonic()
  if mode in ['deadline', 'immediate']:
    if mode == 'immediate':
      await anyio.Path(peer.root / 'release').write_text('owned release')
    async with ClientSession(timeout=ClientTimeout(total=50)) as client:
      async with client.post(url, json={'command': 'help'}) as response:
        data = await response.json()
        status = response.status
    if mode == 'deadline':
      assert status == 504 and data == {'ok': False, 'error': 'command timed out'}
      assert not await anyio.Path(peer.root / 'capture-finished').exists()
    else:
      assert status == 200 and data == {'ok': True, 'rc': 0, 'out': 'owned output \ufffd', 'error': 'owned error \ufffd'}
    observed = {'status': status, 'body': data, 'seconds': time.monotonic() - began}
  else:
    stream = await anyio.connect_tcp('127.0.0.1', peer.port)
    body = b'{"command":"help"}'
    await stream.send(
      f'POST /api/terminal_commands/run HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n'.encode() + body
    )
    await anyio.to_thread.run_sync(wait_path, peer.root / 'capture-child.json', True)
    if mode == 'force':
      assert peer.process and peer.process.stdin and peer.reader
      await peer.process.stdin.send(b'force\n')
      wire = bytearray()
      with anyio.fail_after(5):
        try:
          while True:
            wire.extend(await stream.receive())
        except anyio.EndOfStream:
          cleanup = json.loads(await peer.reader.receive_until(b'\n', 65536))
        await peer.process.wait()
      assert cleanup['terminal_reaped'] and peer.process.returncode == 0
      assert not await anyio.Path(peer.root / 'capture-finished').exists()
      observed = {'force_cleanup': cleanup, 'force_wire': wire.decode(errors='replace'), 'seconds': time.monotonic() - began}
      await stream.aclose()
    else:
      await stream.aclose()
      await anyio.Path(peer.root / 'release').write_text('owned release')
      await anyio.to_thread.run_sync(wait_path, peer.root / 'capture-finished', True)
      observed = {'continued_after_disconnect': True, 'seconds': time.monotonic() - began}
  child = json.loads(await anyio.Path(peer.root / 'capture-child.json').read_text())
  await anyio.to_thread.run_sync(wait_path, Path(f'/proc/{child["pid"]}'), False)
  observed['owned_child'] = child
  observed['child_reaped'] = True
  await anyio.to_thread.run_sync(save, peer.root / 'capture-observation.json', observed)
  return observed


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  inputs = Inputs(args.native.resolve(), args.launcher.resolve(), args.binding.resolve(), args.vision_root.resolve())
  mode = 'deadline' if args.timeout else 'force' if args.force else 'immediate' if args.immediate else 'disconnect'
  names = ['native'] if args.native_only or args.force else ['source', 'native']
  peers = [Peer(root / name, name == 'source', inputs, args.server.resolve(), True) for name in names]
  try:
    for peer in peers:
      await peer.start()
    async with anyio.create_task_group() as tasks:
      for peer in peers:
        tasks.start_soon(exercise, peer, mode)
    await anyio.to_thread.run_sync(
      save,
      root / 'result.json',
      {
        'mode': mode,
        'providers': names,
        'actual_45_second_capture_deadline': args.timeout,
        'owned_child_reaped': True,
        'disconnect_continuation': mode == 'disconnect',
      },
    )
  finally:
    original = sys.exception()
    errors = []
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
        raise ExceptionGroup('capture fixture cleanup', errors)


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['server', 'native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--timeout', action='store_true')
  parser.add_argument('--immediate', action='store_true')
  parser.add_argument('--force', action='store_true')
  parser.add_argument('--native-only', action='store_true')
  args = parser.parse_args()
  await anyio.Path(args.output.resolve()).mkdir(parents=True)
  await run(args)


if __name__ == '__main__':
  anyio.run(main)

#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import resource
import sys

import anyio
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Inputs
from carrot_server_terminal_http_fixture import Peer


async def allocation() -> None:
  source_modules()
  from openpilot.selfdrive.carrot.server.services.terminal_pty import PersistentPtySession

  session = PersistentPtySession()
  baseline = len(os.listdir('/proc/self/fd'))
  previous = resource.getrlimit(resource.RLIMIT_NOFILE)
  resource.setrlimit(resource.RLIMIT_NOFILE, (64, previous[1]))
  files = []
  try:
    while True:
      try:
        files.append(os.open('/dev/null', os.O_RDONLY))
      except OSError as error:
        assert error.errno == 24
        break
    try:
      await session.ensure()
    except OSError as error:
      observed = error.errno
      message = str(error)
    else:
      raise AssertionError('allocation unexpectedly succeeded')
  finally:
    for fd in files:
      os.close(fd)
    resource.setrlimit(resource.RLIMIT_NOFILE, previous)
  after = len(os.listdir('/proc/self/fd'))
  print(json.dumps({'errno': observed, 'error': message, 'before': baseline, 'after': after, 'source_actual_ensure': True}))
  assert after == baseline


async def failed_start(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  fake = root / 'malformed-ready'
  await anyio.Path(fake).write_text(f'#!{sys.executable}\nimport time\nprint("malformed-json",flush=True)\ntime.sleep(30)\n')
  await anyio.Path(fake).chmod(0o700)
  peer = Peer(root / 'partial', False, Inputs(args.native, args.launcher, args.binding, args.vision_root), fake, False)
  before = await anyio.to_thread.run_sync(lambda: len(os.listdir('/proc/self/fd')))
  original = ''
  cleanup_errors = []
  try:
    await peer.start()
  except json.JSONDecodeError as error:
    original = str(error)
  finally:
    with anyio.CancelScope(shield=True):
      try:
        await peer.close()
      except (OSError, TimeoutError, ExceptionGroup) as error:
        cleanup_errors.append(str(error))
  assert original and peer.process and peer.process.returncode is not None
  assert not await anyio.Path(f'/proc/{peer.process.pid}').exists()
  after = await anyio.to_thread.run_sync(lambda: len(os.listdir('/proc/self/fd')))
  assert after == before
  await anyio.to_thread.run_sync(
    save,
    root / 'result.json',
    {
      'original_start_error': original,
      'owned_pid': peer.process.pid,
      'exit': peer.process.returncode,
      'before_fd': before,
      'after_fd': after,
      'cleanup_errors': cleanup_errors,
    },
  )


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--allocation', action='store_true')
  for name in ['native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path)
  args = parser.parse_args()
  if args.allocation:
    await allocation()
  else:
    assert all([args.native, args.launcher, args.binding, args.vision_root, args.output])
    await failed_start(args)


if __name__ == '__main__':
  anyio.run(main)

#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Held synchronous capture: bounded source signal observation and explicit native force ownership."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import sys
import time

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_tools_fixture import Fixture, Options
from supervision_peer import enable_subreaper


async def run(args: argparse.Namespace) -> None:
  enable_subreaper()
  fixture = Fixture(Options(args.output.resolve(), args.native.resolve(), args.launcher.resolve(), args.binding.resolve(), force=True))
  held = []
  try:
    await fixture.start()
    for peer in fixture.peers:
      notice = peer.root / 'notice.pipe'
      blocked = peer.root / 'held.pipe'
      await anyio.to_thread.run_sync(os.mkfifo, notice)
      await anyio.to_thread.run_sync(os.mkfifo, blocked)
      script = '\n'.join(
        [
          f'#!{sys.executable}',
          'import json,os',
          'from pathlib import Path',
          'identity={"pid":os.getpid(),"sid":os.getsid(0),"starttime":Path("/proc/self/stat").read_text().rsplit(")",1)[1].split()[19]}',
          f'Path({str(peer.root / "held-child.json")!r}).write_text(json.dumps(identity))',
          f'with open({str(notice)!r}, "wb", buffering=0) as stream: stream.write(b"1")',
          f'with open({str(blocked)!r}, "rb") as stream: stream.read()',
          '',
        ]
      )
      await anyio.Path(peer.root / 'bin/tmux').write_text(script)
      await anyio.Path(peer.root / 'bin/tmux').chmod(0o755)
      reader = await anyio.to_thread.run_sync(lambda path=notice: os.open(path, os.O_RDONLY | os.O_NONBLOCK))
      try:
        async with await anyio.connect_tcp('127.0.0.1', peer.port) as stream:
          body = b'{"action":"send_tmux_log"}'
          header = f'POST /api/tools HTTP/1.1\r\nHost: localhost\r\nContent-Length: {len(body)}\r\n\r\n'
          await stream.send(header.encode() + body)
          with anyio.fail_after(4):
            await anyio.wait_readable(reader)
            assert os.read(reader, 1) == b'1'
          child = json.loads(await anyio.Path(peer.root / 'held-child.json').read_text())
          held.append(child)
        assert peer.process and peer.process.stdin
        assert child['sid'] == os.getsid(peer.process.pid)
        start = time.monotonic()
        await peer.process.stdin.send(b'\n')
        await peer.process.stdin.aclose()
        if peer.name == 'source':
          await anyio.sleep(0.2)
          assert peer.process.returncode is None
          assert await anyio.Path(f'/proc/{child["pid"]}').exists()
          peer.process.terminate()
          with anyio.fail_after(3):
            await peer.process.wait()
          assert peer.process.returncode == -signal.SIGTERM
          child_alive = await anyio.Path(f'/proc/{child["pid"]}').exists()
          assert child_alive
          os.kill(child['pid'], signal.SIGKILL)
          pid, status = await anyio.to_thread.run_sync(os.waitpid, child['pid'], 0)
          assert pid == child['pid'] and os.WIFSIGNALED(status)
          # Expected fixture termination was observed and reaped; ordinary peer
          # cleanup still closes its streams without interpreting it as success.
        else:
          with anyio.fail_after(3):
            await peer.process.wait()
          assert peer.process.returncode == 0
          assert not await anyio.Path(f'/proc/{child["pid"]}').exists()
        await anyio.to_thread.run_sync(
          save,
          peer.root / 'force-observation.json',
          {
            'child': child,
            'server_exit': peer.process.returncode,
            'seconds': time.monotonic() - start,
            'caller_disconnected_before_stop': True,
            'child_absent_after_owned_cleanup': not await anyio.Path(f'/proc/{child["pid"]}').exists(),
            'source_timer_blocked': peer.name == 'source',
            'native_direct_force': peer.name == 'native',
          },
        )
      finally:
        os.close(reader)
    await anyio.to_thread.run_sync(
      save,
      fixture.options.output / 'result.json',
      {
        'source_blocked_timer_and_child_survives_parent_signal': True,
        'native_force_direct_child_reaped': True,
        'global_60_second_window_unchanged': 'existing server deadline reused; this fixture explicitly invokes force after quiescing',
        'process_groups_or_descendants_killed': False,
      },
    )
  finally:
    errors = []
    for peer in fixture.peers:
      errors.extend(await peer.close())
    errors = [error for error in errors if error != 'source exit -15']
    for child in held:
      path = anyio.Path(f'/proc/{child["pid"]}/stat')
      if await path.exists() and (await path.read_text()).rsplit(')', 1)[1].split()[19] == child['starttime']:
        os.kill(child['pid'], signal.SIGKILL)
        try:
          await anyio.to_thread.run_sync(os.waitpid, child['pid'], 0)
        except ChildProcessError:
          print(f"fixture child already reaped: {child['pid']}", file=sys.stderr)
    await anyio.to_thread.run_sync(save, fixture.options.output / 'cleanup.json', {'errors': errors})
    assert not errors, errors


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'native', 'launcher', 'binding']:
    parser.add_argument('--' + name, type=Path, required=True)
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

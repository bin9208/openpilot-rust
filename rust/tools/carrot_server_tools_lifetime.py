#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Accepted Tools job survives its caller and is observed at actual App/runtime shutdown."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time
import uuid
from typing import assert_never

from carrot_server_dashcam_sync_probe import Json

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import request, save


async def run(args: argparse.Namespace) -> None:
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  root = output / ('native' if args.native else 'source')
  for name in ['state', 'repository', 'media', 'owned-params', 'bin']:
    await anyio.Path(root / name).mkdir(parents=True, exist_ok=True)
  if args.application:
    await anyio.Path(root / 'web').mkdir()
    await anyio.Path(root / 'web/index.html').write_text('owned Tools lifetime application')
    await anyio.Path(root / 'settings.json').write_text('{}')
  prefix = 'tools-' + uuid.uuid4().hex
  notify, blocked = root / 'notice.pipe', root / 'blocked.pipe'
  await anyio.to_thread.run_sync(os.mkfifo, notify)
  await anyio.to_thread.run_sync(os.mkfifo, blocked)
  wrapper = '\n'.join(
    [
      f'#!{sys.executable}',
      'import os, json, sys',
      'from pathlib import Path',
      f'Path({str(root / "child.json")!r}).write_text(json.dumps({{"pid": os.getpid(), "sid": os.getsid(0)}}))',
      f'with open({str(notify)!r}, "wb", buffering=0) as notice: notice.write(b"1")',
      'os.execv("/usr/bin/cat", ["cat", *sys.argv[1:]])',
      '',
    ]
  )
  await anyio.Path(root / 'bin/cat').write_text(wrapper)
  await anyio.Path(root / 'bin/cat').chmod(0o755)
  argv = [str(args.native.resolve())] if args.native else [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_tools_source.py'))]
  config = {'owned_root': str(root), 'launcher': str(args.launcher.resolve()) if args.launcher else '', 'application': args.application}
  environment = os.environ | {
    'PATH': str(root / 'bin') + os.pathsep + os.environ['PATH'],
    'PARAMS_ROOT': str(root / 'owned-params'),
    'OPENPILOT_PREFIX': prefix,
    'CARROT_DATA_DIR': str(root),
    'ORIGINAL_PARAMS_BINDING': str(await anyio.Path(args.binding).resolve()),
  }
  process = None
  child = None
  reader_fd = await anyio.to_thread.run_sync(lambda: os.open(notify, os.O_RDONLY | os.O_NONBLOCK))
  log = await anyio.Path(root / 'process.log').open('wb')
  try:
    process = await anyio.open_process(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log.wrapped)
    assert process.stdin and process.stdout
    await process.stdin.send((json.dumps(config) + '\n').encode())
    with anyio.fail_after(6):
      ready = json.loads(await BufferedByteReceiveStream(process.stdout).receive_until(b'\n', 65536))
    body = json.dumps({'action': 'shell_cmd', 'cmd': 'cat ' + shlex.quote(str(blocked))}).encode()
    started = await request(ready['port'], '/api/tools/start', 'POST', body)
    assert started['status'] == 200 and started['payload']['status'] == 'running'
    with anyio.fail_after(3):
      await anyio.wait_readable(reader_fd)
      assert os.read(reader_fd, 1) == b'1'
    child = json.loads(await anyio.Path(root / 'child.json').read_text())
    assert child['pid'] == child['sid']
    job = await request(ready['port'], '/api/tools/job?id=' + started['payload']['job_id'])
    assert job['payload']['status'] == 'running' and await anyio.Path(f'/proc/{child["pid"]}').exists()
    await anyio.to_thread.run_sync(save, root / 'accepted.json', {'start': started, 'job_after_caller_closed': job, 'owned_child': child})
    stopped = time.monotonic()
    await process.stdin.send(b'\n')
    await process.stdin.aclose()
    with anyio.fail_after(4):
      await process.wait()
    assert process.returncode == 0 and not await anyio.Path(f'/proc/{child["pid"]}').exists()
    persisted = json.loads(await anyio.Path(root / 'state/tool_jobs.json').read_text())
    cleanup: dict[str, Json] | list[Json] = json.loads(await anyio.Path(root / 'app-cleanup.json').read_text())
    match cleanup:
      case dict():
        cleanup_jobs = cleanup['jobs']
      case list():
        cleanup_jobs = cleanup
      case _:
        assert_never(cleanup)
    reloaded = None
    if args.native:
      await process.aclose()
      process = await anyio.open_process(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log.wrapped)
      assert process.stdin and process.stdout
      await process.stdin.send((json.dumps(config) + '\n').encode())
      with anyio.fail_after(6):
        ready = json.loads(await BufferedByteReceiveStream(process.stdout).receive_until(b'\n', 65536))
      reloaded = await request(ready['port'], '/api/tools/job?id=' + started['payload']['job_id'])
      assert reloaded['payload']['status'] == 'failed'
      assert reloaded['payload']['error'] == 'server restarted before job completed'
      await process.stdin.send(b'\n')
      await process.stdin.aclose()
      with anyio.fail_after(4):
        await process.wait()
      assert process.returncode == 0
    await anyio.to_thread.run_sync(
      save,
      output / 'result.json',
      {
        'start_returned_before_job_completed': True,
        'continued_after_caller_close': True,
        'app_cleanup_job_status': cleanup_jobs[0]['status'],
        'persisted_job_status': persisted['jobs'][0]['status'],
        'owned_group_reaped_at_runtime_exit': True,
        'seconds': time.monotonic() - stopped,
        'child': child,
        'reload': reloaded,
        'process_exit': process.returncode,
      },
    )
  finally:
    os.close(reader_fd)
    with anyio.CancelScope(shield=True):
      if process:
        if process.returncode is None:
          process.kill()
        await process.wait()
        await process.aclose()
      if child and await anyio.Path(f'/proc/{child["pid"]}').exists():
        try:
          os.killpg(child['pid'], 9)
        except ProcessLookupError:
          print(f"fixture child exited before fallback signal: {child['pid']}", file=sys.stderr)
      await log.aclose()
  await anyio.to_thread.run_sync(
    save,
    output / 'invocation.json',
    {
      'argv': argv,
      'input': config,
      'environment': {name: environment[name] for name in ['PATH', 'PARAMS_ROOT', 'OPENPILOT_PREFIX', 'CARROT_DATA_DIR', 'ORIGINAL_PARAMS_BINDING']},
    },
  )


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'binding']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--application', action='store_true')
  parser.add_argument('--native', type=Path)
  parser.add_argument('--launcher', type=Path)
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

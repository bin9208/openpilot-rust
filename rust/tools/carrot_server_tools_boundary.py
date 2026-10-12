#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Focused Tools route, policy, retained lock, strict stream and timeout boundaries."""

from __future__ import annotations

import argparse
import base64
import fcntl
import json
from pathlib import Path

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import request, save
from carrot_server_dashcam_sync_probe import Json
from carrot_server_tools_fixture import Fixture, Options
from carrot_server_dashcam_media import read_response


async def routes(fixture: Fixture) -> None:
  cases = [
    ('missing-action', '/api/tools', 'POST', b'{}'),
    ('false-action', '/api/tools/start', 'POST', b'{"action":false}'),
    ('unknown-action', '/api/tools', 'POST', b'{"action":[1]}'),
    ('invalid-json', '/api/tools/start', 'POST', b'{'),
    ('scalar', '/api/tools', 'POST', b'[]'),
    ('scalar-start', '/api/tools/start', 'POST', b'null'),
    ('job-no-id', '/api/tools/job', 'GET', b''),
    ('job-missing', '/api/tools/job?id=missing&id=other', 'GET', b''),
    ('job-head', '/api/tools/job?id=missing', 'HEAD', b''),
    ('job-post', '/api/tools/job', 'POST', b''),
    ('jobs-empty', '/api/tools/jobs?limit=bad', 'GET', b''),
    ('jobs-head', '/api/tools/jobs', 'HEAD', b''),
    ('notice-missing', '/api/tools/jobs/notice', 'POST', b'{"message":false}'),
    ('notice-scalar', '/api/tools/jobs/notice', 'POST', b'3'),
    ('notice-normal', '/api/tools/jobs/notice', 'POST', b'{"message":true,"action":"  owned notice  "}'),
    ('jobs-first-limit', '/api/tools/jobs?limit=1&limit=bad', 'GET', b''),
    ('clear-finished', '/api/tools/jobs', 'DELETE', b''),
    ('device-info', '/api/tools/device_info', 'GET', b''),
  ]
  for label, path, method, body in cases:
    observations = [await request(peer.port, path, method, body) for peer in fixture.peers]
    await anyio.to_thread.run_sync(save, fixture.options.output / f'{label}.json', {'source': observations[0], 'native': observations[1]})
    values = [fixture.normalized(item['payload'], peer) for item, peer in zip(observations, fixture.peers, strict=True)]
    assert observations[0]['status'] == observations[1]['status'] and values[0] == values[1], (label, values)
    for field in ['content-type', 'allow']:
      assert observations[0]['headers'].get(field) == observations[1]['headers'].get(field), (label, field)
    fixture.cases.append(label)


async def policy(fixture: Fixture) -> None:
  commands: list[dict[str, Json]] = [
    {'action': 'shell_cmd'},
    {'action': 'shell_cmd', 'cmd': '"bad'},
    {'action': 'shell_cmd', 'cmd': '""'},
    {'action': 'shell_cmd', 'cmd': 'rm owned'},
    {'action': 'shell_cmd', 'cmd': 'git'},
    {'action': 'shell_cmd', 'cmd': 'git commit'},
    {'action': 'shell_cmd', 'cmd': 'pull'},
    {'action': 'shell_cmd', 'cmd': 'cat absent-owned-file'},
    {'action': 'git_reset', 'mode': 'bad'},
    {'action': 'git_checkout'},
    {'action': 'git_remote_set'},
    {'action': 'git_remote_add'},
    {'action': 'git_reset_repo_checkout'},
  ]
  for job in [False, True]:
    for index, command in enumerate(commands):
      await fixture.pair(f'{"job" if job else "sync"}-policy-{index}', command, job)
    for peer in fixture.peers:
      await anyio.Path(peer.root / 'tmux-fail').touch()
    await fixture.pair(f'{"job" if job else "sync"}-tmux-failure', {'action': 'send_tmux_log'}, job)


async def lock(fixture: Fixture) -> None:
  files = [await anyio.Path(peer.root / 'repository.lock').open('a+b') for peer in fixture.peers]
  try:
    for stream in files:
      await anyio.to_thread.run_sync(fcntl.flock, stream.wrapped.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    await fixture.pair('sync-git-busy', {'action': 'git_reset'})
    await fixture.pair('job-git-busy', {'action': 'git_reset'}, True)
    await fixture.pair('alias-not-repo-locked', {'action': 'shell_cmd', 'cmd': 'status --short'})
    await fixture.pair('raw-git-repo-locked', {'action': 'shell_cmd', 'cmd': 'git status --short'})
  finally:
    for stream in files:
      await stream.aclose()


async def strict(fixture: Fixture) -> None:
  for peer in fixture.peers:
    await anyio.Path(peer.root / 'repository/invalid-bytes').write_bytes(b'good\xffbad')
  await fixture.pair('sync-strict-utf8', {'action': 'shell_cmd', 'cmd': 'cat invalid-bytes'})
  await fixture.pair('job-replacement-utf8', {'action': 'shell_cmd', 'cmd': 'cat invalid-bytes'}, True)


async def timeout(fixture: Fixture, sync_only: bool = False) -> None:
  for peer in fixture.peers:
    await anyio.Path(peer.root / 'bin/cat').write_text('#!/bin/sh\nexec /bin/sleep 60\n')
    await anyio.Path(peer.root / 'bin/cat').chmod(0o755)
  # The real source ten-second shell deadline is the changed boundary; run peers concurrently.
  observations: list[dict[str, Json]] = [{}, {}]

  async def call(index: int) -> None:
    peer = fixture.peers[index]
    body = b'{"action":"shell_cmd","cmd":"cat"}'
    async with await anyio.connect_tcp('127.0.0.1', peer.port) as stream:
      headers = f'POST /api/tools HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {len(body)}\r\n\r\n'
      await stream.send(headers.encode() + body)
      with anyio.fail_after(13):
        result = await read_response(BufferedByteReceiveStream(stream), 'POST')
      result['payload'] = json.loads(base64.b64decode(result['body_base64']))
      observations[index] = result

  async with anyio.create_task_group() as tasks:
    for index in range(2):
      tasks.start_soon(call, index)
  await anyio.to_thread.run_sync(save, fixture.options.output / 'sync-timeout.json', observations)
  assert observations[0]['status'] == observations[1]['status'] == 504
  assert observations[0]['payload'] == observations[1]['payload'] == {'ok': False, 'error': 'timeout'}
  fixture.cases.append('sync-timeout')
  if sync_only:
    return
  # Job dispatch already returns before the deadline; preserve its actual log/result.
  await fixture.pair('job-timeout', {'action': 'shell_cmd', 'cmd': 'cat'}, True)


async def run(args: argparse.Namespace) -> None:
  fixture = Fixture(Options(args.output.resolve(), args.native.resolve(), args.launcher.resolve(), args.binding.resolve()))
  try:
    await fixture.start()
    if args.surrogates:
      for endpoint in ['/api/tools', '/api/tools/start']:
        observations = [await request(peer.port, endpoint, 'POST', b'{"action":"\\ud800"}') for peer in fixture.peers]
        await anyio.to_thread.run_sync(save, fixture.options.output / ('start.json' if endpoint.endswith('start') else 'sync.json'), observations)
        assert observations[0]['payload'] == observations[1]['payload'], observations
        fixture.cases.append(endpoint)
    elif args.utf8:
      failures = []
      for label, data in [('truncated', b'\xe2\x82'), ('continuation', b'\xe2A')]:
        for peer in fixture.peers:
          await anyio.Path(peer.root / 'repository/invalid-bytes').write_bytes(data)
        try:
          await fixture.pair(label, {'action': 'shell_cmd', 'cmd': 'cat invalid-bytes'})
        except AssertionError as error:
          failures.append(str(error))
      await anyio.to_thread.run_sync(save, fixture.options.output / 'comparisons.json', {'failures': failures})
      assert not failures, failures
    elif args.sync_timeout:
      await timeout(fixture, True)
    elif args.unknown_git:
      files = [await anyio.Path(peer.root / 'repository.lock').open('a+b') for peer in fixture.peers]
      try:
        for stream in files:
          await anyio.to_thread.run_sync(fcntl.flock, stream.wrapped.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        await fixture.pair('unknown-git-lock-priority', {'action': 'git_owned_unknown'})
      finally:
        for stream in files:
          await stream.aclose()
      await fixture.pair('unknown-git-after-preflight', {'action': 'git_owned_unknown'})
    elif args.tmux_send_error:
      for peer in fixture.peers:
        await anyio.Path(peer.root / 'owned-params/d/CarrotException').mkdir()
      for job in [False, True]:
        await fixture.pair('tmux-send-write-failure-' + ('job' if job else 'sync'), {'action': 'server_tmux_log'}, job)
    elif args.tmux:
      for job in [False, True]:
        label = 'job' if job else 'sync'
        await fixture.pair(label + '-tmux', {'action': 'send_tmux_log'}, job)
        for peer in fixture.peers:
          await anyio.Path(peer.root / 'media/tmux.log').unlink()
          await anyio.Path(peer.root / 'media/tmux.log').mkdir()
        await fixture.pair(label + '-tmux-directory', {'action': 'send_tmux_log'}, job)
        for peer in fixture.peers:
          await anyio.Path(peer.root / 'media/tmux.log').rmdir()
    elif args.timeout:
      await timeout(fixture)
    else:
      await routes(fixture)
      await policy(fixture)
      await lock(fixture)
      await strict(fixture)
    await fixture.result()
  finally:
    await fixture.close()


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'native', 'launcher', 'binding']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--timeout', action='store_true')
  parser.add_argument('--surrogates', action='store_true')
  parser.add_argument('--utf8', action='store_true')
  parser.add_argument('--sync-timeout', action='store_true')
  parser.add_argument('--tmux', action='store_true')
  parser.add_argument('--tmux-send-error', action='store_true')
  parser.add_argument('--unknown-git', action='store_true')
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

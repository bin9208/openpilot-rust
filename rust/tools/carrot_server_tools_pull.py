#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Actual changed-HEAD summaries and synchronous/streaming dirty-checkout split."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

import anyio
from carrot_server_dashcam_upload import request, save
from carrot_server_tools_fixture import Fixture, Options


async def advance(fixture: Fixture, index: int) -> None:
  root = fixture.options.output
  await anyio.Path(root / 'seed/owned.txt').write_text(f'owned revision {index}\n')
  environment = os.environ | {'GIT_AUTHOR_DATE': f'2000-01-0{index + 1}T00:00:00Z', 'GIT_COMMITTER_DATE': f'2000-01-0{index + 1}T00:00:00Z'}
  await anyio.run_process(['git', 'commit', '-am', f'Owned update {index}'], cwd=root / 'seed', env=environment)
  await anyio.run_process(['git', 'push', 'origin', 'dev'], cwd=root / 'seed')


async def run(args: argparse.Namespace) -> None:
  fixture = Fixture(Options(args.output.resolve(), args.native.resolve(), args.launcher.resolve(), args.binding.resolve()))
  try:
    await fixture.start()
    await advance(fixture, 1)
    await fixture.pair('sync-updated-summary', {'action': 'git_pull'})
    for peer in fixture.peers:
      path = anyio.Path(peer.root / 'state/git.json')
      state = json.loads(await path.read_text()) if await path.exists() else {}
      if peer.name == 'source':
        assert state['git_pull_time']
      else:
        assert not state.get('git_pull_time'), 'Rejected native origin must not report an update'
      await anyio.to_thread.run_sync(save, peer.root / 'pull-state.json', state)
    await advance(fixture, 2)
    for peer in fixture.peers:
      await anyio.Path(peer.root / 'repository/owned.txt').write_text('owned dirty file\n')
    await fixture.pair('sync-preserves-dirty-file', {'action': 'git_pull'})
    for peer in fixture.peers:
      assert await anyio.Path(peer.root / 'repository/owned.txt').read_text() == 'owned dirty file\n'
    await fixture.pair('job-resets-dirty-and-updates', {'action': 'git_pull'}, True)
    for peer in fixture.peers:
      expected = 'owned revision 2\n' if peer.name == 'source' else 'owned dirty file\n'
      assert await anyio.Path(peer.root / 'repository/owned.txt').read_text() == expected
    await fixture.pair('job-already-up-to-date-summary', {'action': 'git_pull'}, True)
    # The original cases above retain their observations. At the owned package
    # boundary, successful native staging must still leave active source alone.
    native = fixture.peers[1]
    repository = native.root / 'repository'
    await anyio.run_process(['/usr/bin/git', 'remote', 'set-url', 'origin', 'https://github.com/bin9208/openpilot-rust.git'], cwd=repository)
    helper = repository / 'rust/bin/openpilot-runtime-install'
    await anyio.Path(helper).write_text('#!/bin/sh\nprintf \'%s\\n\' "$@" > package-attempt\necho "Owned verified package staged"\nexit 0\n')
    before_head = (await anyio.run_process(['/usr/bin/git', 'rev-parse', 'HEAD'], cwd=repository)).stdout
    before_source = await anyio.Path(repository / 'owned.txt').read_bytes()
    successes = []
    for command in [
      {'action': 'git_checkout', 'kind': 'remote', 'remote': 'origin', 'name': 'dev'},
      {'action': 'git_reset', 'mode': 'hard', 'target': 'HEAD'},
      {'action': 'git_reset_repo_checkout', 'branch': 'dev'},
      {'action': 'git_pull'},
      {'action': 'rebuild_all'},
    ]:
      response = await request(native.port, '/api/tools', 'POST', json.dumps(command).encode())
      value = response['payload']
      assert response['status'] == 200 and value['ok'] is True and value['staged'] is True, response
      if command['action'] == 'rebuild_all':
        assert value['rebooting'] is True
      else:
        assert value['reboot_required'] is True
      assert (await anyio.run_process(['/usr/bin/git', 'rev-parse', 'HEAD'], cwd=repository)).stdout == before_head
      assert await anyio.Path(repository / 'owned.txt').read_bytes() == before_source
      arguments = (await anyio.Path(repository / 'package-attempt').read_text()).splitlines()
      assert arguments[:3] == ['--stage-update', '--root', str(repository)]
      successes.append({'command': command, 'response': response, 'active_head_unchanged': True, 'active_source_unchanged': True, 'helper_arguments': arguments})
    await anyio.to_thread.run_sync(save, fixture.options.output / 'native-staged-success.json', successes)
    await fixture.result()
  finally:
    await fixture.close()


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'native', 'launcher', 'binding']:
    parser.add_argument('--' + name, type=Path, required=True)
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

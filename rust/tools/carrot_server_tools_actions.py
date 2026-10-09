#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Complete active Tools sync/job action family against owned providers."""

from __future__ import annotations

import argparse
from pathlib import Path

import anyio
from carrot_server_tools_fixture import Fixture, Options


async def files(fixture: Fixture, job: bool) -> None:
  label = 'job' if job else 'sync'
  for peer in fixture.peers:
    for path in [
      'media/0/videos/owned.mp4',
      'media/0/videos/.hidden',
      'media/0/realdata/owned.log',
      'media/0/realdata/.hidden',
      'owned-params/d/CalibrationParams',
      'owned-params/d_tmp/CalibrationParams',
      'repository/prebuilt',
    ]:
      await anyio.Path(peer.root / path).write_text('owned fixture\n')
    await anyio.Path(peer.root / 'media/0/realdata/subdirectory').mkdir(exist_ok=True)
  for action in ['delete_all_videos', 'delete_all_logs', 'send_tmux_log', 'server_tmux_log', 'backup_settings', 'reset_calib', 'reboot', 'rebuild_all']:
    await fixture.pair(label + '-' + action, {'action': action}, job)
  # Provider calls are asynchronous source Popen operations; let the owned one-second calibration child finish before cleanup.
  await anyio.sleep(1.1)
  for peer in fixture.peers:
    assert await anyio.Path(peer.root / 'media/0/videos/.hidden').exists()
    assert not await anyio.Path(peer.root / 'media/0/videos/owned.mp4').exists()
    assert not await anyio.Path(peer.root / 'media/0/realdata/subdirectory').exists()
    assert await anyio.Path(peer.root / 'media/tmux.log').read_text() == 'owned pane\nsecond line\n'
    assert await anyio.Path(peer.root / 'owned-params/d/CarrotException').read_text() == 'tmux_send'


async def git(fixture: Fixture, job: bool) -> None:
  label = 'job' if job else 'sync'
  operations = [
    {'action': 'git_log', 'count': 4},
    {'action': 'git_branch_list'},
    {'action': 'git_remote_add', 'name': 'second', 'url': '<remote>'},
    {'action': 'git_remote_set', 'url': '<remote>'},
    {'action': 'git_reset', 'mode': 'soft', 'target': 'HEAD'},
    {'action': 'git_checkout', 'kind': 'remote', 'name': 'topic', 'remote': 'origin'},
    {'action': 'git_checkout', 'kind': 'local', 'name': 'dev'},
    {'action': 'git_sync'},
    {'action': 'git_pull'},
    {'action': 'git_reset_repo_fetch'},
    {'action': 'git_reset_repo_checkout', 'branch': 'dev'},
    {'action': 'shell_cmd', 'cmd': 'echo "owned text" "single quoted"'},
  ]
  for index, command in enumerate(operations):
    await fixture.pair(f'{label}-git-{index}-{command["action"]}', command, job)


async def run(args: argparse.Namespace) -> None:
  options = Options(args.output.resolve(), args.native.resolve(), args.launcher.resolve(), args.binding.resolve())
  fixture = Fixture(options)
  try:
    await fixture.start()
    for job in [False] if args.sync_only else [False, True]:
      await files(fixture, job)
      await git(fixture, job)
    await fixture.result()
  finally:
    await fixture.close()


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'native', 'launcher', 'binding']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--sync-only', action='store_true')
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

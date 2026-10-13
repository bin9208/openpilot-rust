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
from carrot_server_dashcam_upload import save
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
      state = json.loads(await anyio.Path(peer.root / 'state/git.json').read_text())
      assert state['git_pull_time']
      await anyio.to_thread.run_sync(save, peer.root / 'pull-state.json', state)
    await advance(fixture, 2)
    for peer in fixture.peers:
      await anyio.Path(peer.root / 'repository/owned.txt').write_text('owned dirty file\n')
    await fixture.pair('sync-preserves-dirty-file', {'action': 'git_pull'})
    for peer in fixture.peers:
      assert await anyio.Path(peer.root / 'repository/owned.txt').read_text() == 'owned dirty file\n'
    await fixture.pair('job-resets-dirty-and-updates', {'action': 'git_pull'}, True)
    for peer in fixture.peers:
      assert await anyio.Path(peer.root / 'repository/owned.txt').read_text() == 'owned revision 2\n'
    await fixture.pair('job-already-up-to-date-summary', {'action': 'git_pull'}, True)
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

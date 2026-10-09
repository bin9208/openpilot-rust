#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
from pathlib import Path

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Fixture, Inputs


async def compare(root: Path, inputs: Inputs) -> None:
  fixtures = [Fixture(root / name, name, inputs) for name in ['source', 'native']]
  comparisons = []
  try:
    for fixture in fixtures:
      await fixture.setup()
    cases = [
      ['vision', 'invalid'],
      ['vision', 'logs', '--lines', 'abc'],
      ['vision', 'logs', '--lines'],
      ['vision', 'logs', '--unknown'],
      ['--', 'help'],
      ['--bogus'],
      ['--line'],
      ['--line=web-intro'],
      ['vision', 'logs', '--lin=1'],
      ['vision', 'logs', '--lines', '-3'],
    ]
    for args in cases:
      pair = [await fixture.command(args) for fixture in fixtures]
      normalized = [
        [
          row['rc'],
          row['out'].replace(str(fixture.root), '$OWNED'),
          row['error'].replace(Path(row['argv'][2] if fixture.provider == 'source' else row['argv'][0]).name, '$PROGRAM'),
        ]
        for row, fixture in zip(pair, fixtures, strict=True)
      ]
      comparisons.append({'args': args, 'source': pair[0], 'native': pair[1], 'equal': normalized[0] == normalized[1]})
      await anyio.to_thread.run_sync(save, root / 'comparisons.json', comparisons)
      assert normalized[0] == normalized[1], comparisons[-1]
    await anyio.to_thread.run_sync(save, root / 'result.json', {'pairs': len(comparisons), 'all_equal': True, 'program_basename_only_normalized': True})
  finally:
    with anyio.CancelScope(shield=True):
      for fixture in fixtures:
        await anyio.to_thread.run_sync(fixture.cleanup)


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  root = args.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  await compare(root, Inputs(args.native.resolve(), args.launcher.resolve(), args.binding.resolve(), args.vision_root.resolve()))


if __name__ == '__main__':
  anyio.run(main)

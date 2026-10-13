#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python -P rust/tools/carrot_server_terminal_cli.py --native PATH --launcher PATH
#   --binding PATH --vision-root DIR --output DIR
# ──────────────────
"""Actual seven-command comparison and detached vision runtime/cleanup proof."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import time

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Fixture, Inputs


def normalized(value: str, fixture: Fixture) -> str:
  value = value.replace(str(fixture.root), '$OWNED')
  value = re.sub(r'pid=\d+', 'pid=$PID', value)
  value = re.sub(r'elapsed=\d\d:\d\d:\d\d', 'elapsed=$ELAPSED', value)
  config = json.loads(fixture.config.read_text())
  return value.replace(str(config['port']), '$PORT')


async def run(output: Path, inputs: Inputs, start_case: int) -> None:
  fixtures = [Fixture(output / name, name, inputs) for name in ['source', 'native']]
  comparisons = []
  try:
    for fixture in fixtures:
      await fixture.setup()
    cases = [
      [],
      ['help'],
      ['help', 'vision'],
      ['help', 'vision_on'],
      ['help', 'unknown'],
      ['unknown'],
      ['--line', "'"],
      ['--line', '\\'],
      ['--line', '   '],
      ['--line', "web-intro"],
      ['web-intro', 'extra'],
      ['web-lab'],
      ['web-lab', 'on'],
      ['web-lab', 'status'],
      ['web-lab', 'disable'],
      ['web-lab', 'status'],
      ['web-lab', 'invalid'],
      ['vision'],
      ['vision', '--help'],
      ['vision', 'logs', '--lines', '1'],
      ['vision', 'status'],
      ['youtube-test', 'help'],
      ['vision_off'],
      ['vision_on'],
      ['vision', 'status'],
      ['vision', 'start'],
      ['vision_off'],
    ]
    for args in cases[start_case:]:
      rows = []
      for fixture in fixtures:
        result = await fixture.command(args)
        rows.append((result['rc'], normalized(result['out'], fixture), normalized(result['error'], fixture)))
        if args == ['vision_on']:
          assert result['rc'] == 0, result
          state = await fixture.state()
          assert state['status'] == 'running' and len(state['children']) == 3
          pid = state['runner_pid']
          assert os.getsid(pid) == pid
          assert await anyio.Path(f'/proc/{pid}/fd/0').readlink() == Path('/dev/null')
          assert await anyio.Path(fixture.params / fixture.prefix / 'IsTakingSnapshot').read_bytes() == b'1'
          await anyio.to_thread.run_sync(
            save, fixture.root / 'normal-running.json', {'state': state, 'sid_equals_pid': True, 'runner_stdin_null': True, 'starter_exited_log_survives': True}
          )
        if args == ['vision_off'] and await anyio.Path(fixture.root / 'vision-state.json').exists():
          await fixture.settled()
      comparisons.append({'args': args, 'source': rows[0], 'native': rows[1], 'equal': rows[0] == rows[1]})
      await anyio.to_thread.run_sync(save, output / 'comparisons.json', comparisons)
      assert rows[0] == rows[1], comparisons[-1]
    for fixture in fixtures:
      await anyio.Path(fixture.params / fixture.prefix / 'IsOffroad').write_bytes(b'0')
    rows = [await fixture.command(['vision_on']) for fixture in fixtures]
    assert [(row['rc'], row['out'], row['error']) for row in rows][0] == [(row['rc'], row['out'], row['error']) for row in rows][1]
    for fixture in fixtures:
      await anyio.Path(fixture.params / fixture.prefix / 'IsOffroad').write_bytes(b'1')
      await anyio.Path(fixture.root / 'stream_encoderd').unlink()
      result = await fixture.command(['vision_on'])
      assert result['rc'] == 1 and 'No such file or directory' in result['error']
      await fixture.settled()
    states = [await fixture.state() for fixture in fixtures]
    assert states[0]['status'] == states[1]['status'] == 'error'
    assert normalized(states[0]['error'], fixtures[0]) == normalized(states[1]['error'], fixtures[1])
    await anyio.to_thread.run_sync(
      save,
      output / 'result.json',
      {
        'paired_cli': len(cases) - start_case + 2,
        'all_equal': True,
        'normal_vision_real_vipc_and_owned_port': True,
        'spawn_failure_terminal_error_unmasked': states,
        'post_stop_and_failure_cleanup_before_fixture_close': True,
      },
    )
  finally:
    for fixture in fixtures:
      await anyio.to_thread.run_sync(fixture.cleanup)


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--start-case', type=int, default=0)
  for name in ['native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  options = parser.parse_args()
  output = options.output.resolve()
  free = await anyio.to_thread.run_sync(lambda: shutil.disk_usage('.').free)
  assert free >= 25 * 1024**3 + 4608 * 1024**2
  await anyio.Path(output).mkdir(parents=True)
  began = time.monotonic()
  await run(output, Inputs(options.native.resolve(), options.launcher.resolve(), options.binding.resolve(), options.vision_root.resolve()), options.start_case)
  await anyio.to_thread.run_sync(
    save,
    output / 'invocation.json',
    {'seconds': time.monotonic() - began, 'native': str(options.native), 'binding': str(options.binding), 'vision_root': str(options.vision_root)},
  )


if __name__ == '__main__':
  anyio.run(main)

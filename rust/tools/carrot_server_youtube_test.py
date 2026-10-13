#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Full original/native offroad CLI start/status/logs/stop and one-shot verification."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_youtube_test_fixture import Fixture, Inputs


async def run(args: argparse.Namespace) -> None:
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  resolved = {}
  for name in ['service', 'cli', 'launcher', 'vision_root', 'frames', 'certificate', 'key', 'binding']:
    resolved[name] = Path(str(await anyio.Path(getattr(args, name)).resolve()))
  os.environ['ORIGINAL_PARAMS_BINDING'] = str(resolved['binding'])
  inputs = Inputs(
    resolved['service'], resolved['cli'], resolved['launcher'], resolved['vision_root'], resolved['frames'], (resolved['certificate'], resolved['key'])
  )
  observations = []
  for provider in args.provider or ['source', 'native']:
    fixture = Fixture(output / provider, provider, inputs)
    try:
      await fixture.setup()
      if args.affected_cleanup:
        scenarios = args.scenario or ['verify-cleanup', 'startup-failure']
        observation = {'provider': provider}
        if 'verify-cleanup' in scenarios:
          await verify_cleanup(fixture)
          observation.update(verify_post_cleanup=True, vipc_console_zero=True)
        if 'startup-failure' in scenarios:
          await startup_failure(fixture)
          observation['encoder_spawn_failure_cleanup'] = True
        observations.append(observation)
        continue
      for label, key, value, reason in [
        ('offroad-guard', 'IsOffroad', b'0', 'device is not offroad'),
        ('snapshot-guard', 'IsTakingSnapshot', b'1', 'another camera test is active'),
        ('caption-guard', 'CarrotYouTubeTimestamp', b'2', 'disable CarrotYouTubeTimestamp first'),
      ]:
        await anyio.Path(fixture.peer.params / fixture.peer.prefix / key).write_bytes(value)
        result = await fixture.command(label, ['start'])
        assert result['exit'] == 1 and reason in result['stderr']
        await anyio.Path(fixture.peer.params / fixture.peer.prefix / key).write_bytes(b'1' if key == 'IsOffroad' else b'0')
      started = await fixture.command('start', ['start'])
      assert started['exit'] == 0, started
      state = json.loads(await anyio.Path(fixture.root / 'test-state.json').read_text())
      pid = state['runner_pid']
      session = await anyio.to_thread.run_sync(os.getsid, pid)
      stdin = await anyio.Path(f'/proc/{pid}/fd/0').readlink()
      assert session == pid and str(stdin) == '/dev/null'
      assert state['source_sample']['header_bytes'] > 0 and state['source_sample']['width'] == 854
      initial = (await anyio.Path(fixture.root / 'test.log').stat()).st_size
      await anyio.sleep(0.8)
      assert await anyio.Path(f'/proc/{pid}/cmdline').exists()
      log = await anyio.Path(fixture.root / 'test.log').read_text()
      assert '[owned-youtube-child] encoderd ready' in log
      await anyio.to_thread.run_sync(
        save,
        fixture.root / 'detached-runner.json',
        {'pid': pid, 'sid': session, 'stdin': str(stdin), 'starter_exited': True, 'log_initial_bytes': initial, 'log_read_after_starter_exit': log},
      )
      status = await fixture.command('status', ['status'])
      assert status['exit'] == 0
      logs = await fixture.command('logs', ['logs', '--lines=2'])
      assert logs['exit'] == 0
      stopped = await fixture.command('stop', ['stop'])
      assert stopped['exit'] == 0
      assert await anyio.Path(fixture.peer.params / fixture.peer.prefix / 'CarrotYouTubeLive').read_bytes() == b'0'
      assert await anyio.Path(fixture.peer.params / fixture.peer.prefix / 'IsTakingSnapshot').read_bytes() == b'0'
      for child in ['camerad', 'encoderd']:
        child_id = json.loads(await anyio.Path(fixture.root / (child + '-started.json')).read_text())['pid']
        assert not await anyio.Path(f'/proc/{child_id}').exists()
      verified = await fixture.command('verify', ['verify'])
      assert verified['exit'] == 0 and 'verification passed (10s stable)' in verified['stdout'], verified
      report = json.loads(await anyio.Path(fixture.root / 'test-report.json').read_text())
      assert report['diagnosis']['healthy'] and report['diagnosis']['verdict'] == 'pass'
      await anyio.to_thread.run_sync(save, fixture.root / 'verified-report.json', report)
      observations.append(
        {
          'provider': provider,
          'startup': True,
          'detached_sid_stdin_log': True,
          'owned_children_reaped': True,
          'verify_seconds': verified['seconds'],
          'diagnosis': report['diagnosis'],
        }
      )
    finally:
      with anyio.CancelScope(shield=True):
        await fixture.close()
  if not args.affected_cleanup:
    assert observations[0]['diagnosis'] == observations[1]['diagnosis']
  await anyio.to_thread.run_sync(
    save, output / 'result.json', {'providers': observations, 'source_bodies_unchanged': True, 'owned_vipc_cereal_media_network': True}
  )


async def verify_cleanup(fixture: Fixture) -> None:
  verified = await fixture.command('verify', ['verify'])
  assert verified['exit'] == 0 and 'VIPC streams     0' in verified['stdout'], verified
  state = json.loads(await anyio.Path(fixture.root / 'test-state.json').read_text())
  await exited(state['runner_pid'])
  assert state['status'] == 'stopped' and state['children'] == {}
  assert not await anyio.Path(f'/proc/{state["runner_pid"]}').exists()
  children = []
  for name in ['camerad', 'encoderd']:
    child = json.loads(await anyio.Path(fixture.root / (name + '-started.json')).read_text())
    assert not await anyio.Path(f'/proc/{child["pid"]}').exists()
    children.append(child)
  for name in ['CarrotYouTubeLive', 'IsTakingSnapshot']:
    assert await anyio.Path(fixture.peer.params / fixture.peer.prefix / name).read_bytes() == b'0'
  await anyio.to_thread.run_sync(
    save,
    fixture.root / 'post-verify-cleanup.json',
    {
      'before_fixture_close': True,
      'runner': state['runner_pid'],
      'runner_absent': True,
      'children': children,
      'children_absent': True,
      'live_snapshot_restored': True,
    },
  )


async def startup_failure(fixture: Fixture) -> None:
  await anyio.Path(fixture.root / 'encoderd').unlink()
  failed = await fixture.command('encoder-missing', ['start'])
  assert failed['exit'] == 1
  state = json.loads(await anyio.Path(fixture.root / 'test-state.json').read_text())
  await exited(state['runner_pid'])
  state = json.loads(await anyio.Path(fixture.root / 'test-state.json').read_text())
  assert state['status'] == 'error' and state['children'] == {}
  for name in ['CarrotYouTubeLive', 'IsTakingSnapshot']:
    assert await anyio.Path(fixture.peer.params / fixture.peer.prefix / name).read_bytes() == b'0'
  child = json.loads(await anyio.Path(fixture.root / 'camerad-started.json').read_text())
  assert not await anyio.Path(f'/proc/{child["pid"]}').exists()
  await anyio.to_thread.run_sync(
    save,
    fixture.root / 'startup-failure-cleanup.json',
    {'before_fixture_close': True, 'state': state, 'camera': child, 'camera_absent': True, 'live_snapshot_restored': True},
  )


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'service', 'cli', 'launcher', 'vision_root', 'frames', 'certificate', 'key', 'binding']:
    parser.add_argument('--' + name.replace('_', '-'), dest=name, type=Path, required=True)
  parser.add_argument('--affected-cleanup', action='store_true')
  parser.add_argument('--provider', choices=['source', 'native'], action='append')
  parser.add_argument('--scenario', choices=['verify-cleanup', 'startup-failure'], action='append')
  anyio.run(run, parser.parse_args())


async def exited(pid: int) -> None:
  try:
    descriptor = os.pidfd_open(pid)
  except ProcessLookupError:
    return
  try:
    with anyio.fail_after(5):
      await anyio.wait_readable(descriptor)
  finally:
    os.close(descriptor)


if __name__ == '__main__':
  main()

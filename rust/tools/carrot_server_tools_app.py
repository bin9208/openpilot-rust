#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Real native Application routing with original Tools/Params handlers and owned static fallback."""

from __future__ import annotations

import argparse
import base64
import json
from pathlib import Path

import anyio
from carrot_server_dashcam_upload import request, save
from carrot_server_tools_fixture import Fixture, Options


async def run(args: argparse.Namespace) -> None:
  fixture = Fixture(Options(args.output.resolve(), args.native.resolve(), args.launcher.resolve(), args.binding.resolve(), True, legacy=args.legacy))
  try:
    await fixture.start()
    if args.legacy:
      rows = [await request(peer.port, '/api/tools/job?id=legacy-owned') for peer in fixture.peers]
      await anyio.to_thread.run_sync(save, fixture.options.output / 'migration.json', rows)
      expected = fixture.normalized(rows[0]['payload'], fixture.peers[0])
      actual = fixture.normalized(rows[1]['payload'], fixture.peers[1])
      assert expected == actual and actual['status'] == 'failed'
      assert actual['error'] == 'server restarted before job completed'
      fixture.cases.append('migrate-before-job-load')
      await fixture.result()
      return
    await fixture.pair('app-sync', {'action': 'shell_cmd', 'cmd': 'echo owned App'})
    await fixture.pair('app-job', {'action': 'shell_cmd', 'cmd': 'echo owned job'}, True)
    await fixture.pair('app-backup', {'action': 'backup_settings'})
    downloaded = []
    for peer in fixture.peers:
      heartbeat = await request(peer.port, '/api/heartbeat_status')
      await anyio.to_thread.run_sync(save, peer.root / 'heartbeat.json', heartbeat)
      assert heartbeat['status'] == 200 and heartbeat['payload'] == {'ok': True, 'hb': {'ok': None, 'msg': 'not yet', 'ts': 0}}
      download = await request(peer.port, '/download/params_backup.json')
      assert download['status'] == 200
      raw = base64.b64decode(download['body_base64'])
      assert not raw.endswith(b'\n')
      values = json.loads(raw)
      downloaded.append(values)
      assert values['CustomSR'] == '12'
      # Reuse actual converted validated restore routes, selecting one owned scalar.
      await anyio.Path(peer.root / 'owned-params/d/CustomSR').write_text('15')
      body = json.dumps({'values': values, 'keys': ['CustomSR']}).encode()
      preview = await request(peer.port, '/api/params_restore_preview', 'POST', body)
      restored = await request(peer.port, '/api/params_restore_json', 'POST', body)
      await anyio.to_thread.run_sync(save, peer.root / 'backup-restore.json', {'download': download, 'preview': preview, 'restored': restored})
      assert preview['status'] == restored['status'] == 200 and restored['payload']['ok']
      assert await anyio.Path(peer.root / 'owned-params/d/CustomSR').read_text() == '12'
      head = await request(peer.port, '/api/tools/jobs', 'HEAD')
      await anyio.to_thread.run_sync(save, peer.root / 'jobs-head.json', head)
      assert head['status'] == 200 and not head['body_base64']
    assert downloaded[0] == downloaded[1]
    fixtures = [json.loads(await anyio.Path(peer.root / 'backup-restore.json').read_text()) for peer in fixture.peers]
    for key in ['preview', 'restored']:
      assert fixtures[0][key]['payload'] == fixtures[1][key]['payload'], (key, fixtures)
    fixture.cases.extend(['heartbeat', 'download-backup', 'restore-preview', 'restore-apply', 'jobs-head'])
    await fixture.result()
  finally:
    await fixture.close()


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['output', 'native', 'launcher', 'binding']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--legacy', action='store_true')
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()

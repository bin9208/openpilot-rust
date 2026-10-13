#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_qr_dependency.py BINARY BUNDLE NEW_OUTPUT
"""Check native provider repair and original/native QR HTTP behavior on owned roots."""

from __future__ import annotations

import base64
from collections.abc import Sequence
import json
import os
from pathlib import Path
import shutil
import sys
from typing import Final, Literal, TypeAlias, assert_never

import anyio
from carrot_server_dashcam_sync_probe import Json, Peer, startup
from carrot_server_system_actions import BINDING, ROOT
from carrot_server_system_http import fetch

SOURCE: Final = Path(__file__).with_name('carrot_server_qr_dependency_source.py')
LIBRARIES: Final = ('libbrotlienc.so.1', 'libbrotlidec.so.1', 'libbrotlicommon.so.1')
Scenario: TypeAlias = Literal['corrupt', 'target', 'version', 'symlink', 'extra', 'missing']
SCENARIOS: Final[tuple[Scenario, ...]] = ('corrupt', 'target', 'version', 'symlink', 'extra', 'missing')


def save(path: Path, value: Json) -> None:
  path.write_text(json.dumps(value, indent=2) + '\n')


async def commands(binary: Path, root: Path, bundle: Path, actions: Sequence[Json]) -> list[Json]:
  config = {'root': str(root), 'bundle': str(bundle), 'system_fallback': False}
  request = '\n'.join(json.dumps(value) for value in [config, *actions]) + '\n'
  env = os.environ | {'PARAMS_ROOT': str(root / 'params'), 'OPENPILOT_PREFIX': 'd'}
  with anyio.fail_after(8):
    result = await anyio.run_process([str(binary)], input=request.encode(), env=env, check=False)
  record = {'config': config, 'actions': list(actions), 'exit': result.returncode, 'stdout': result.stdout.decode(), 'stderr': result.stderr.decode()}
  await anyio.to_thread.run_sync(save, root / 'invocation.json', record)
  assert result.returncode == 0, record
  return [json.loads(line) for line in result.stdout.splitlines()]


def altered(bundle: Path, destination: Path, scenario: Scenario) -> None:
  shutil.copytree(bundle, destination)
  manifest = json.loads((destination / 'manifest.json').read_text())
  match scenario:
    case 'corrupt':
      (destination / LIBRARIES[0]).write_bytes(b'corrupt')
    case 'target':
      manifest['target'] = 'wrong-target'
    case 'version':
      manifest['brotli_version'] += 1
    case 'symlink':
      (destination / LIBRARIES[0]).unlink()
      (destination / LIBRARIES[0]).symlink_to(bundle / LIBRARIES[0])
    case 'extra':
      manifest['files']['../escape'] = '0' * 64
    case 'missing':
      (destination / LIBRARIES[1]).unlink()
    case unexpected:
      assert_never(unexpected)
  (destination / 'manifest.json').write_text(json.dumps(manifest))


async def repair(binary: Path, bundle: Path, output: Path) -> None:
  scenarios: list[Json] = []
  values = {'IsMetric': '1', 'CarName': 'Owned QR'}
  root = output / 'repair'
  await anyio.Path(root).mkdir()
  rows = await commands(
    binary,
    root,
    bundle,
    [
      {'action': 'status'},
      {'action': 'build', 'values': values},
      {'action': 'ensure'},
      {'action': 'ensure'},
      {'action': 'inspect', 'values': values},
    ],
  )
  await anyio.to_thread.run_sync(save, root / 'responses.json', rows)
  assert rows[0]['installed'] is False and rows[0]['format'] == 'CQR4'
  assert rows[1]['payload'].startswith('CQR4:')
  assert rows[2]['ok'] is True and rows[2]['configured'] is True and rows[2]['installed'] is True
  assert rows[3]['configured'] is False and rows[3]['message'] == 'already installed'
  assert rows[4]['qr']['payload'].startswith('CQR3:')
  generation = Path(await anyio.Path(root / 'native-deps/brotli/current').resolve(strict=True))
  assert all(str(generation / name) in rows[4]['maps'] for name in LIBRARIES)
  parsed = await commands(binary, root, bundle, [{'action': 'parse', 'payload': rows[4]['qr']['payload']}])
  assert parsed == [values]
  scenarios.append({'case': 'fresh-repair-idempotence-cqr3-roundtrip', 'passed': True, 'generation': str(generation)})
  for name in SCENARIOS:
    case = output / name
    await anyio.Path(case).mkdir()
    candidate = case / 'bundle'
    await anyio.to_thread.run_sync(altered, bundle, candidate, name)
    rows = await commands(binary, case, candidate, [{'action': 'ensure'}, {'action': 'build', 'values': values}])
    await anyio.to_thread.run_sync(save, case / 'responses.json', rows)
    assert rows[0]['ok'] is False and rows[0]['installed'] is False and rows[0]['configured'] is False
    assert 'module_path' not in rows[0] and rows[1]['payload'].startswith('CQR4:')
    active = case / 'native-deps/brotli'
    assert not await anyio.Path(active / 'current').exists()
    entries = [item.name async for item in anyio.Path(active).iterdir()]
    assert all(not entry.startswith(('generation-', '.activation-')) for entry in entries)
    scenarios.append({'case': name, 'passed': True})
  await anyio.to_thread.run_sync(save, output / 'repair-result.json', scenarios)


def payload(response: Json) -> Json:
  return json.loads(base64.b64decode(response['body_base64']))


async def application(binary: Path, bundle: Path, output: Path, *, system: bool) -> None:
  peers: list[Peer] = []
  responses: list[Json] = []
  try:
    for name, command in [('source', [sys.executable, '-P', str(SOURCE)]), ('native', [str(binary)])]:
      root = output / name
      await anyio.Path(root).mkdir(parents=True)
      await anyio.Path(root / 'openpilot/selfdrive/carrot/web').mkdir(parents=True)
      await anyio.Path(root / 'openpilot/selfdrive/assets').mkdir(parents=True)
      peer = Peer(root)
      peers.append(peer)
      env = os.environ | {'PARAMS_ROOT': str(root / 'params'), 'OPENPILOT_PREFIX': 'd', 'CARROT_DATA_DIR': str(root), 'CARROT_LOG_ROOT': str(root / 'logs')}
      config = {'root': str(root), 'binding': str(BINDING), 'bundle': str(bundle), 'system_fallback': system, 'http': True}
      await startup(peer, peer.start(command, config, env, True))
      await anyio.Path(root / 'params/d/IsMetric').write_bytes(b'1')
    before = await fetch(peers[1], '/api/params_qr_backup')
    assert payload(before)['payload'].startswith('CQR3:' if system else 'CQR4:')
    for path, method in [
      ('/api/params_qr_dependency', 'GET'),
      ('/api/params_qr_dependency/ensure', 'POST'),
      ('/api/params_qr_dependency', 'HEAD'),
      ('/api/params_qr_dependency', 'POST'),
      ('/api/params_qr_dependency/ensure', 'GET'),
      ('/api/params_qr_backup', 'GET'),
    ]:
      source = await fetch(peers[0], path, method)
      native = await fetch(peers[1], path, method)
      record = {'path': path, 'method': method, 'source': source, 'native': native}
      responses.append(record)
      await anyio.to_thread.run_sync(save, output / 'responses.json', responses)
      assert source['status'] == native['status']
      if method == 'HEAD':
        assert source['body_base64'] == native['body_base64'] == ''
      elif source['status'] == 405:
        assert source['headers']['allow'] == native['headers']['allow']
      elif path == '/api/params_qr_backup':
        assert payload(source) == payload(native)
      else:
        wanted, got = payload(source), payload(native)
        assert all(wanted[key] == got[key] for key in ['ok', 'dependency'])
        if not system and method == 'GET':
          assert wanted['installed'] is True and wanted['format'] == 'CQR3'
          assert got['installed'] is False and got['format'] == 'CQR4' and got['module_path'] == ''
        else:
          assert all(wanted[key] == got[key] for key in ['installed', 'format'])
          assert await anyio.Path(wanted['module_path']).is_file() and await anyio.Path(got['module_path']).is_file()
        assert got['provider'] == 'native-brotli'
        if method == 'POST':
          assert wanted['configured'] is False and got['configured'] is (not system)
    await anyio.to_thread.run_sync(save, output / 'responses.json', responses)
  finally:
    for peer in peers:
      await peer.close()
  await anyio.to_thread.run_sync(save, output / 'result.json', {'pairs': len(responses), 'passed': True, 'system_fallback': system})


async def unavailable_http(binary: Path, output: Path) -> None:
  await anyio.Path(output).mkdir(parents=True)
  await anyio.Path(output / 'openpilot/selfdrive/carrot/web').mkdir(parents=True)
  await anyio.Path(output / 'openpilot/selfdrive/assets').mkdir(parents=True)
  peer = Peer(output)
  env = os.environ | {'PARAMS_ROOT': str(output / 'params'), 'OPENPILOT_PREFIX': 'd', 'CARROT_DATA_DIR': str(output)}
  config = {'root': str(output), 'bundle': str(output / 'missing-bundle'), 'system_fallback': False, 'http': True}
  responses: list[Json] = []
  try:
    await startup(peer, peer.start([str(binary)], config, env, True))
    for path, method, status in [
      ('/api/params_qr_dependency', 'GET', 200),
      ('/api/params_qr_dependency/ensure', 'POST', 500),
      ('/api/params_qr_backup', 'GET', 200),
    ]:
      response = await fetch(peer, path, method)
      responses.append(response)
      await anyio.to_thread.run_sync(save, output / 'responses.json', responses)
      assert response['status'] == status
      value = payload(response)
      if path == '/api/params_qr_backup':
        assert value['payload'].startswith('CQR4:')
      else:
        assert value['installed'] is False and value['format'] == 'CQR4'
        if method == 'POST':
          assert value['ok'] is False and value['configured'] is False and 'module_path' not in value
  finally:
    await peer.close()
  await anyio.to_thread.run_sync(save, output / 'result.json', {'observations': 3, 'passed': True})


async def main(binary: Path, bundle: Path, output: Path, prior: Path | None = None) -> None:
  await anyio.Path(output).mkdir(parents=True)
  if prior is None:
    await repair(binary, bundle, output)
  else:
    rows = json.loads(await anyio.Path(prior / 'repair-result.json').read_text())
    assert len(rows) == 7 and all(row['passed'] for row in rows)
    await anyio.to_thread.run_sync(save, output / 'repair-reuse.json', {'artifact': str(prior / 'repair-result.json'), 'cases': 7})
  for name, system in [('system', True), ('repaired', False)]:
    directory = output / name
    await anyio.Path(directory).mkdir()
    await application(binary, bundle, directory, system=system)
  await unavailable_http(binary, output / 'unavailable')
  await anyio.to_thread.run_sync(
    save, output / 'result.json', {'repair_cases': 7, 'http_pairs': 12, 'native_http_failure': 3, 'passed': True, 'source': str(ROOT)}
  )
  print('PASS')


if __name__ == '__main__':
  prior = Path(sys.argv[4]).resolve() if len(sys.argv) == 5 else None
  anyio.run(main, Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(), Path(sys.argv[3]).resolve(), prior)

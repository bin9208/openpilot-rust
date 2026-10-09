import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType

from aiohttp import ClientSession, web
from yarl import URL
from original_params_binding import load
from carrot_server_params_http_cases import cases as param_cases, prepare as prepare_params
from carrot_server_bootstrap_http_cases import cases as bootstrap_cases, prepare as prepare_bootstrap, register as register_bootstrap
import carrot_server_profiles_http_cases as profiles_http
import carrot_server_restore_http_cases as restore_http


def preference_cases():
  units, favorites = '/api/setting_unit_index', '/api/setting_favorites'
  return {
    'units-empty': ('GET', units, None),
    'units-put': ('POST', units, {'units': {' B ': 2, 'A': '3', 'empty': 0, 'invalid': 'x'}}),
    'units-merge': ('POST', units, {'units': {'C': 1, 'A': 0, 'B': 'invalid', 'D': 99}}),
    'units-head': ('HEAD', units, None),
    'units-non-object': ('POST', units, ['ignored']),
    'units-invalid-members': ('POST', units, {'units': ['not a mapping']}),
    'units-overflow': ('POST', units, {'units': {'A': float('inf')}}),
    'units-malformed': ('POST', units, b'{bad'),
    'units-limit': ('POST', units, {'units': {f'P{index}': 1 for index in range(405)}}),
    'units-method': ('PUT', units, {}),
    'units-surrogate': ('POST', units, {'units': {'x\ud800y': 1}}),
    'units-corrupt': ('GET', units, None),
    'units-recovered': ('POST', units, {'units': {'복구': 5, '\x1c trimmed \x1f': 1}}),
    'favorites-empty': ('GET', favorites, None),
    'favorites-put': ('POST', favorites, {'favorites': [' A ', 'A', '', None, False, 0, True, ['nested'], '한글']}),
    'favorites-ignore': ('POST', favorites, {'ignored': True}),
    'favorites-non-object': ('POST', favorites, None),
    'favorites-malformed': ('POST', favorites, b'{bad'),
    'favorites-limit': ('POST', favorites, {'favorites': [f'P{index}' for index in range(205)]}),
    'favorites-head': ('HEAD', favorites, None),
    'favorites-surrogate': ('POST', favorites, {'favorites': ['x\ud800y']}),
    'favorites-corrupt': ('GET', favorites, None),
    'favorites-recovered': ('POST', favorites, {'favorites': ['복구', '\x1c trimmed \x1f']}),
    'favorites-latin1': ('POST', favorites, b'{"favorites": ["caf\xe9\x80"]}'),
    'web-empty': ('GET', '/api/web_settings', None),
    'web-malformed': ('POST', '/api/web_settings', b'{bad'),
    'web-non-object': ('POST', '/api/web_settings', []),
    'web-legacy': ('GET', '/api/web_settings', None),
    'web-update': ('POST', '/api/web_settings', {'web_lab_enabled': True, 'vision_ar_enabled': True, 'web_language': 'MAIN_ZH-Chs', 'carrot_navi_horizontal_area_2': 'vision', 'carrot_navi_split_ratio': 0.501, 'unknown': True}),
    'web-head': ('HEAD', '/api/web_settings', None),
    'web-surrogate': ('POST', '/api/web_settings', {'kmap_url': 'x\ud800y'}),
    'web-recovered': ('POST', '/api/web_settings', {'kmap_url': 'https://restored.example/'}),
  }


async def compare(binary: Path, output: Path, binding: Path, family: str):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-server-owned-') as temporary:
    root = Path(temporary)
    os.environ['CARROT_DATA_DIR'] = str(root / 'data')
    load(binding, f'ipc://{root}/source-log.sock', output / 'binding-logs')
    # Load the real settings handler without the full-app route registrar's optional dependencies.
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.common.params import Params
    from openpilot.selfdrive.carrot.server.features import settings as feature
    from openpilot.selfdrive.carrot.server.features import setting_favorites as favorites_feature
    from openpilot.selfdrive.carrot.server.features import web_settings as web_feature
    from openpilot.selfdrive.carrot.server.features import params as params_feature
    from openpilot.selfdrive.carrot.server.services import param_changes as changes
    changes.time.time = lambda: 1700000000
    from openpilot.selfdrive.carrot.server.services import params as source_params
    from openpilot.selfdrive.carrot.server.services import settings as source_settings
    from openpilot.selfdrive.carrot.server.services import static_assets as source_assets
    from openpilot.selfdrive.carrot.server.services import web_settings as source_web
    if not source_params.HAS_PARAMS:
      raise RuntimeError('original native Params binding is required for this owned HTTP comparison')
    if family == 'restore-unavailable':
      params_feature.HAS_PARAMS = False
    settings_file = root / 'settings.json'
    definition = {'apilot': 'owned', 'params': [
      {'name': 'CruiseGapLevels', 'group': 'Gap', 'min': 2, 'max': 4, 'default': 4,
       'options': {'ko': ['2', '3', '4']}},
      {'name': 'visible', 'group': 'Z'},
      {'name': 'hidden', 'group': 'Z', 'hidden_brands': ['hyundai']},
    ]}
    settings_file.write_text(json.dumps(definition))
    for name in ('web', 'shared_assets', 'training_assets', 'legacy_state', 'data', 'native_data'):
      (root / name).mkdir()
    (root / 'web/js').mkdir()
    catalog = root / 'web/src/features/drive/core/content_catalog.json'
    catalog.parent.mkdir(parents=True)
    catalog.write_bytes((Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/web/src/features/drive/core/content_catalog.json').read_bytes())
    source_web.DRIVE_CONTENT_CATALOG_PATH = str(catalog)
    asset = root / 'web/js/app.js'
    asset.write_bytes(b'export const fixture = 1;\n')
    os.utime(asset, (1700000123, 1700000123))
    (root / 'outside.js').write_text('outside the web root')
    fingerprint = hashlib.sha256(asset.read_bytes()).hexdigest()
    original_params = Params(str(root / 'source_params'))
    original_params.put('CarName', 'HYUNDAI FIXTURE')
    native_params = Params(str(root / 'native_params'))
    native_params.put('CarName', 'HYUNDAI FIXTURE')
    source_params.Params = feature.Params = lambda: original_params
    source_settings.settings_cache.update({'path': str(settings_file), 'data': None, 'mtime': 0})
    app = web.Application(client_max_size=16 * 1024 * 1024,
                          middlewares=[source_assets.create_static_cache_middleware(str(root / 'web'))])
    app.router.add_get('/api/settings', feature.api_settings)
    app.router.add_get('/api/setting_unit_index', feature.api_setting_unit_index)
    app.router.add_post('/api/setting_unit_index', feature.api_setting_unit_index_update)
    favorites_feature.register(app)
    web_feature.register(app)
    app.router.add_get('/api/params_bulk', params_feature.api_params_bulk)
    app.router.add_post('/api/param_set', params_feature.api_param_set)
    register_bootstrap(app, root, original_params)
    profiles_http.register(app, root)
    restore_http.register(app, root)
    app.router.add_static('/', str(root / 'web'), show_index=True)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    source_port = site._server.sockets[0].getsockname()[1]
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
    assert child.stdin is not None and child.stdout is not None and child.stderr is not None
    fixture = {name: str(root / name) for name in ('web', 'shared_assets', 'training_assets', 'legacy_state', 'data')}
    fixture.update(repository=str(root), settings=str(settings_file), params=str(root / 'native_params'))
    fixture['data'] = str(root / 'native_data')
    fixture['timestamp'] = 1700000000
    fixture['cars'] = str(root / 'cars')
    fixture['params_backup'] = str(root / 'params_backup.json')
    fixture['unavailable'] = family == 'restore-unavailable'
    child.stdin.write(json.dumps(fixture) + '\n')
    child.stdin.flush()
    try:
      line = await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10)
      native_port = json.loads(line)['port']
      rows = {'original': [], 'native': []}
      async with ClientSession(auto_decompress=False) as session:
        scenarios = ('catalog', 'gap-invalid', 'gap-four', 'brand-change', 'head', 'method', 'asset', 'asset-head', 'asset-cache',
                     'asset-stale', 'asset-double-version', 'asset-gzip', 'asset-missing',
                     'asset-outside', 'missing', 'malformed', 'recovered')
        preferences = preference_cases()
        preferences.update(param_cases())
        preferences.update(bootstrap_cases())
        preferences.update(profiles_http.cases())
        scenarios, preferences = restore_http.select(family, scenarios, preferences)
        for index, scenario in enumerate((*scenarios, *preferences)):
          prepare_params(scenario, root, (original_params, native_params), definition)
          prepare_bootstrap(scenario, root, (original_params, native_params))
          profiles_http.prepare(scenario, root)
          restore_http.prepare(scenario, root, (original_params, native_params), definition)
          if scenario in ('gap-invalid', 'gap-four'):
            for params in (original_params, native_params):
              params.put_int('LongitudinalPersonalityMax', 5 if scenario == 'gap-invalid' else 4)
          elif scenario == 'brand-change':
            for params in (original_params, native_params):
              params.put('CarName', 'TOYOTA FIXTURE')
          elif scenario == 'missing':
            settings_file.unlink()
          elif scenario == 'malformed':
            settings_file.write_text('{invalid')
            os.utime(settings_file, (1700000000 + index, 1700000000 + index))
          elif scenario == 'recovered':
            settings_file.write_text(json.dumps(definition))
            os.utime(settings_file, (1700000000 + index, 1700000000 + index))
          method = 'HEAD' if scenario in ('head', 'asset-head') else 'POST' if scenario == 'method' else 'GET'
          path = '/api/settings'
          if scenario.startswith('asset'):
            path = '/js/app.js'
          if scenario == 'asset-cache':
            path += f'?v={fingerprint}'
          elif scenario == 'asset-stale':
            path += '?v=stale'
          elif scenario == 'asset-double-version':
            path += f'?v={fingerprint}&v={fingerprint}'
          elif scenario == 'asset-missing':
            path = '/js/missing.js'
          elif scenario == 'asset-outside':
            path = '/%2e%2e/outside.js'
          coding = 'gzip' if scenario == 'asset-gzip' else 'identity'
          request_body = None
          if scenario in preferences:
            method, path, payload = preferences[scenario]
            request_body = payload if isinstance(payload, bytes) else json.dumps(payload).encode() if method not in ('GET', 'HEAD') else None
          if scenario in ('units-corrupt', 'favorites-corrupt'):
            name = 'setting_unit_index.json' if scenario == 'units-corrupt' else 'setting_favorites.json'
            for directory in ('data', 'native_data'):
              (root / directory / 'state' / name).write_text('{bad')
          if scenario == 'web-legacy':
            for directory in ('data', 'native_data'):
              (root / directory / 'state/web_settings.json').write_text(json.dumps({'toss_upload_url': 'https://shind0.synology.me', 'web_language': 'main_en'}))
          for side, port in (('original', source_port), ('native', native_port)):
            if scenario.startswith('profiles-') and method not in ('GET', 'HEAD'):
              request_body = payload if isinstance(payload, bytes) else json.dumps(profiles_http.payload(side, payload)).encode()
            git_before = profiles_http.marker_count()
            headers = {'Accept-Encoding': coding}
            headers.update(restore_http.headers(scenario, root))
            if scenario.endswith('latin1'):
              headers['Content-Type'] = 'application/json; charset=latin-1'
            async with session.request(method, URL(f'http://127.0.0.1:{port}{path}', encoded=True), headers=headers, data=request_body) as response:
              body = await response.read()
              (output / f'{index}-{side}.body').write_bytes(body)
              (output / f'{index}-{side}.headers.json').write_text(json.dumps(list(response.headers.items())))
              headers = {key: response.headers.get(key) for key in ('Content-Type', 'Content-Length', 'Cache-Control', 'Allow', 'Content-Encoding', 'Last-Modified', 'ETag', 'Accept-Ranges', 'Vary', 'Content-Disposition', 'Content-Range')}
              rows[side].append({'scenario': scenario, 'status': response.status, 'headers': headers, 'body_hex': body.hex()})
              profiles_http.capture(scenario, side, rows[side][-1], git_before, output)
              restore_http.capture(scenario, original_params if side == 'original' else native_params, rows[side][-1], output, index, side)
            if scenario in preferences:
              directory = root / ('data' if side == 'original' else 'native_data') / 'state'
              saved = {name: (directory / name).read_bytes().hex() if (directory / name).is_file() else None
                       for name in ('setting_unit_index.json', 'setting_favorites.json', 'web_settings.json', 'setting_unit_index.json.tmp', 'setting_favorites.json.tmp', 'web_settings.json.tmp', 'param_changes.jsonl', 'intro.json', 'setting_profiles.json', 'setting_profiles.json.tmp', 'fingerprint_baseline.json')}
              (output / f'{index}-{side}.state.json').write_text(json.dumps(saved))
              rows[side][-1]['state'] = saved
            if scenario.startswith(('bulk-', 'set-')):
              store = original_params if side == 'original' else native_params
              saved = {name: Path(store.get_param_path(name)).read_bytes().hex()
                       if Path(store.get_param_path(name)).is_file() else None
                       for name in ('IsMetric', 'CruiseGapLevels', 'FutureSetting', 'UptimeOnroad', 'InstallDate', 'DisableDM')}
              saved['DisableDM-kind'] = 'directory' if Path(store.get_param_path('DisableDM')).is_dir() else 'file' if Path(store.get_param_path('DisableDM')).is_file() else 'absent'
              (output / f'{index}-{side}.params.json').write_text(json.dumps(saved))
              rows[side][-1]['params'] = saved
            rows[side][-1] = profiles_http.normalize(side, rows[side][-1])
      for side in rows:
        (output / f'{side}.json').write_text(json.dumps(rows[side], indent=2) + '\n')
      assert rows['original'] == rows['native'], 'original/native owned HTTP responses differ'
      child.stdin.write('stop\n')
      child.stdin.flush()
      code = await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
      assert code == 0, f'native graceful stop exited {code}'
      (output / 'result.json').write_text(json.dumps({'observations': len(rows['original']), 'graceful_stop': code, 'passed': True}) + '\n')
      print(f'owned HTTP comparison: {len(rows["original"])} observations and graceful stop passed')
    finally:
      await runner.cleanup()
      if child.poll() is None:
        child.terminate()
        try:
          await asyncio.wait_for(asyncio.to_thread(child.wait), 5)
        except TimeoutError:
          child.kill()
          await asyncio.to_thread(child.wait)
      (output / 'native.stderr').write_text(child.stderr.read())


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--family', choices=('checkpoint', 'restore', 'restore-unavailable'), default='checkpoint')
  args = parser.parse_args()
  asyncio.run(compare(args.binary, args.output, args.binding, args.family))


if __name__ == '__main__':
  main()

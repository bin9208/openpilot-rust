#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Reuse the retained original Params/aiohttp environment; no installation is needed.
# python rust/tools/carrot_server_popular_values.py --binary rust/target/debug/examples/carrot-server-popular-values --binding PARAMS_SO --output EVIDENCE
# ──────────────────
from __future__ import annotations

import argparse
import copy
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.cookies import SimpleCookie
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import zlib
from types import ModuleType, SimpleNamespace
from typing import TypeAlias, TypedDict

import aiohttp
from aiohttp import web
import anyio
import brotli
from original_params_binding import load

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']
class Case(TypedDict, total=False):
  operation: str
  scenario: str
  value: Json
  setting: Json
  memory: Json
  car_key: str
  settings_hash: str
  name: str
  now: float
  last: float
  interval: float
  session: bool
  in_flight: bool
  popular: bool
  unavailable: bool
  catalog: Json
  seed: dict[str, str]
  env: dict[str, str]

CATALOG = {'apilot': 20260929, 'params': [
  {'name': 'OwnedPopularBool', 'min': 0, 'max': 1, 'default': 0, 'unit': '켜짐'},
  {'name': 'OwnedPopularInt', 'min': -50, 'max': 100, 'default': 7, 'unit': 'km/h'},
  {'name': 'OwnedPopularFloat', 'min': -0.5, 'max': 3.25, 'default': 0.25, 'unit': 'm/s²🚗'},
  {'name': 'OwnedPopularText', 'default': 'owned synthetic'},
  {'name': 'CruiseGapLevels', 'min': 2, 'max': 4, 'default': 4, 'options': {'ko': ['2', '3', '4']}},
]}
SEED = {'CarSelected3': 'Owned Fixture + 차량', 'DongleId': 'owned-fixture-device', 'HardwareSerial': 'owned-hardware', 'GitRemote': 'git@github.com:owned/fixture.git', 'GitCommit': 'owned-fake-commit', 'LongitudinalPersonalityMax': '4', 'CruiseGapLevels': '1', 'OwnedPopularBool': ' YES ', 'OwnedPopularInt': '-12.75', 'OwnedPopularFloat': '1.125', 'OwnedPopularText': 'synthetic'}
EMPTY = {'ok': True, 'car_key': SEED['CarSelected3'], 'popular_values': {'OwnedPopularInt': {'top_values': [{'value': 12, 'count': 3}]}}}
CF_ID = 'owned-fixture-cf-id'
CF_SECRET = 'owned-fixture-cf-secret'
BASE = '/api/setting_popular_values'

def cases() -> list[Case]:
  rows: list[Case] = [{'operation': 'snapshot', 'scenario': 'typed-real-params-snapshot'}, {'operation': 'snapshot', 'scenario': 'params-unavailable', 'unavailable': True}, {'operation': 'snapshot', 'scenario': 'empty-car-key', 'seed': {'CarSelected3': ''}}, {'operation': 'snapshot', 'scenario': 'empty-catalog', 'catalog': {'params': []}}]
  for version in (None, True, '2026', 'bad', 2.75): rows.append({'operation': 'snapshot', 'scenario': 'settings-version', 'catalog': dict(CATALOG, apilot=version)})
  for value in ('unknown', ' NONE ', 'null', 'UnregisteredDevice', '', 'owned-primary'): rows.append({'operation': 'device_id', 'scenario': 'device-fallback', 'seed': {'DongleId': value, 'HardwareSerial': 'owned-secondary'}})
  rows.append({'operation': 'device_id', 'scenario': 'hostname-fallback', 'seed': {'DongleId': '', 'HardwareSerial': ''}})
  for remote in ('', 'git@github.com:owned/repo.git', 'https://github.com/owned/repo.git/', 'ssh://git@github.com/a/b/repo.git?x=1#f', 'owned/repo', 'single', 'git@other:owned/repo.git', 'https://host/a/repo.git;meta', '\nhttps://host/a/re\tpo.git\n'):
    rows.append({'operation': 'repo_id', 'scenario': 'remote-path', 'value': remote})
  for setting in CATALOG['params'][:4]:
    for value in (None, False, True, 3.8, -12.8, ' YES ', '\u001cYES\u001f', '0', '3.9', 'bad', [], {'x': 1}): rows.append({'operation': 'coerce', 'scenario': 'coercion-boundary', 'value': value, 'setting': setting})
  for memory in (None, [], {}, {'car_key': 'other', 'settings_hash': 'hash'}, {'car_key': 'same', 'settings_hash': 'different'}, {'car_key': '', 'settings_hash': ''}, {'car_key': 'same', 'settings_hash': 'hash', 'source': None, 'ok': False, 'popular_values': None}): rows.append({'operation': 'read', 'scenario': 'memory-context-invalidation', 'memory': memory, 'car_key': 'same', 'settings_hash': 'hash'})
  for memory in ({}, EMPTY, {'ok': None, 'car_key_type': 0, 'car_key': 123, 'settings_hash': '', 'popular_values': []}, {'ok': False, 'popular_values': {'x': []}, 'updated_at': 99, 'extra': 3}): rows.append({'operation': 'store', 'scenario': 'remote-cleaning', 'memory': memory, 'now': 1000.9, 'settings_hash': 'current'})
  for name in ('OwnedPopularInt', 'missing', '', 'x'): rows.append({'operation': 'detail', 'scenario': 'detail-default-or-dictionary', 'memory': dict(EMPTY, popular_values=dict(EMPTY['popular_values'], x=[])), 'name': name})
  for now, last, interval, busy, session in ((59., 0., 60., False, True), (60., 0., 60., False, True), (61., 0., 60., True, True), (61., 0., 60., False, False), (10., 20., 60., False, True), (10., 20., -20., False, True), (20., 20., 0., False, True)):
    rows.append({'operation': 'schedule', 'scenario': 'throttle-clock-boundary', 'now': now, 'last': last, 'interval': interval, 'in_flight': busy, 'session': session})
  for value in ('', 'invalid', '-1', '0', '1.5', ' 2 ', 'NaN', 'Infinity'):
    rows.append({'operation': 'env', 'scenario': 'environment-numeric-fallback', 'env': {key: value for key in ('CARROT_PARAM_VALUE_TIMEOUT_S', 'CARROT_PARAM_VALUE_RETRY_DELAY_S', 'CARROT_PARAM_VALUE_RETRY_COUNT', 'CARROT_PARAM_VALUE_REFRESH_MIN_S')}})
  for popular in (False, True):
    for env in ({'CARROT_PARAM_VALUE_URL': 'http://127.0.0.1:1/owned/'}, {'CARROT_PARAM_VALUE_URL': ' http://127.0.0.1:1/owned ', 'CARROT_PARAM_VALUE_SNAPSHOT_URL': 'http://127.0.0.1:2/s', 'CARROT_PARAM_VALUE_POPULAR_URL': 'http://127.0.0.1:2/p'}): rows.append({'operation': 'endpoint', 'scenario': 'owned-endpoint-precedence', 'popular': popular, 'env': env})
  rows.append({'operation': 'credentials_digest', 'scenario': 'encoded-default-equality'})
  rows.append({'operation': 'credentials_digest', 'scenario': 'owned-env-credential-precedence', 'env': {'CARROT_PARAM_VALUE_CF_ID': CF_ID, 'CARROT_PARAM_VALUE_CF_SECRET': CF_SECRET}})
  return rows

def seed(store, values: dict[str, str]) -> None:
  for name, value in values.items(): Path(store.get_param_path(name)).write_bytes(value.encode())

def save(path: Path, data: Json) -> None:
  path.write_text(json.dumps(data, indent=2, ensure_ascii=True, allow_nan=True) + '\n')

async def main() -> None:
  parser = argparse.ArgumentParser(); parser.add_argument('--binary', type=Path); parser.add_argument('--binding', type=Path, required=True); parser.add_argument('--output', type=Path, required=True); parser.add_argument('--compression-only', action='store_true'); parser.add_argument('--composed-only', action='store_true'); parser.add_argument('--http-only', action='store_true'); parser.add_argument('--cookies-only', action='store_true'); parser.add_argument('--rounding-only', action='store_true'); args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=True)
  load(args.binding.resolve(), f'ipc://{args.output.resolve()}/logs.sock', args.output / 'binding-logs')
  features = ModuleType('openpilot.selfdrive.carrot.server.features'); features.__path__ = [str(Path('openpilot/selfdrive/carrot/server/features').resolve())]; sys.modules[features.__name__] = features
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import popular_values as source, params, settings
  from openpilot.selfdrive.carrot.server.features import setting_popular_values as feature
  roots = [args.output / name for name in ('original-params', 'native-params')]; stores = [Params(str(root.resolve())) for root in roots]; params.Params = source.Params = lambda: stores[0]
  source.socket = SimpleNamespace(gethostname=lambda: 'owned-fixture-host'); source.time = SimpleNamespace(time=lambda: 1000.)
  original_settings = source.get_settings_cached; original_car_key = source._current_car_key; original_hash = source._current_settings_hash; original_async = source.asyncio; observations = []; failures = []; old_env = {key: value for key, value in os.environ.items() if key.startswith('CARROT_PARAM_VALUE_')}
  policy_inputs = [] if args.compression_only or args.composed_only or args.http_only or args.cookies_only or args.rounding_only else cases()
  for index, case in enumerate(policy_inputs):
    for key in list(os.environ):
      if key.startswith('CARROT_PARAM_VALUE_'): del os.environ[key]
    os.environ.update(case.get('env', {})); source.HAS_PARAMS = not case.get('unavailable', False)
    catalog = copy.deepcopy(case.get('catalog', CATALOG)); groups, names, groups_list = settings.group_index(catalog); source.get_settings_cached = lambda: (catalog, groups, names, groups_list)
    for store in stores: seed(store, dict(SEED, **case.get('seed', {})))
    source._popular_values_memory = case.get('memory'); source._current_car_key = lambda: case.get('car_key', SEED['CarSelected3']) if source.HAS_PARAMS else ''; source._current_settings_hash = lambda: case.get('settings_hash', source._settings_hash(catalog, list(names))) if source.HAS_PARAMS else ''
    try:
      operation = case['operation']
      match operation:
        case 'snapshot': value = source.build_snapshot_payload()
        case 'coerce': value = source._coerce_value(case.get('value'), case['setting'])
        case 'device_id': value = source._device_id(stores[0])
        case 'repo_id': value = source._repo_id_from_remote(case['value'])
        case 'read': value = source.read_popular_values_memory()
        case 'store': source.time = SimpleNamespace(time=lambda: case['now']); value = source.store_popular_values_memory(case['memory'])
        case 'detail': value = source.get_popular_value_detail(case['name'])
        case 'schedule':
          def owned_task(coroutine): coroutine.close(); return SimpleNamespace(done=lambda: False)
          previous = SimpleNamespace(done=lambda: not case['in_flight']); source._popular_refresh_task = previous; source._popular_refresh_last_at = case['last']; source.time = SimpleNamespace(time=lambda: case['now']); source.asyncio = SimpleNamespace(create_task=owned_task)
          source.schedule_popular_value_refresh({'http': True if case['session'] else None}, case['interval']); value = source._popular_refresh_task is not previous; source.asyncio = original_async
        case 'endpoint': value = source._popular_url(stores[0]) if case['popular'] else source._snapshot_url(stores[0])
        case 'credentials_digest': cid, secret = source._cf_access_token(stores[0]); value = {'id': hashlib.sha256(cid.encode()).hexdigest(), 'secret': hashlib.sha256(secret.encode()).hexdigest()}
        case 'env': value = {'timeout': max(1., source._env_float('CARROT_PARAM_VALUE_TIMEOUT_S', 4.)), 'delay': max(1., source._env_float('CARROT_PARAM_VALUE_RETRY_DELAY_S', 15.)), 'attempts': max(1, source._env_int('CARROT_PARAM_VALUE_RETRY_COUNT', 5)), 'interval': source._env_float('CARROT_PARAM_VALUE_REFRESH_MIN_S', 60.)}
        case _: raise AssertionError(operation)
      original = {'value': value}
    except (TypeError, ValueError, OverflowError, UnicodeEncodeError) as error: original = {'error': type(error).__name__}
    native = None
    if args.binary:
      input_data = dict(case, catalog=catalog, state=str(args.output.resolve()), root='' if case.get('unavailable') else str(roots[1].resolve()))
      process = subprocess.run([str(args.binary.resolve())], input=json.dumps(input_data) + '\n', text=True, capture_output=True, check=True, timeout=10); native = json.loads(process.stdout)
      if ('error' in original) != ('error' in native) or ('value' in original and json.dumps(original, sort_keys=True) != json.dumps(native, sort_keys=True)): failures.append({'kind': 'policy', 'index': index, 'case': case, 'original': original, 'native': native})
    observations.append({'kind': 'policy', 'index': index, 'scenario': case['scenario'], 'input': case, 'original': original, 'native': native})
  source.get_settings_cached = original_settings; source._current_car_key = original_car_key; source._current_settings_hash = original_hash; source.time = SimpleNamespace(time=lambda: 1000.)
  active = {'index': -1, 'mode': 'success', 'side': 'original', 'phase': ''}; outbound = []; recipient_errors = []; counters = {}; gate = threading.Event(); started = threading.Event(); request_started = threading.Event(); download_finished = threading.Event()
  class Recipient(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    def log_message(self, *_args): return
    def exchange(self, upload: bool) -> None:
      key = (active['side'], active['index']); counters[key] = counters.get(key, 0) + 1; mode = active['mode']; body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
      assert self.headers.get('CF-Access-Client-Id') == CF_ID and self.headers.get('CF-Access-Client-Secret') == CF_SECRET
      outbound.append({'side': active['side'], 'phase': active['phase'], 'mode': mode, 'index': active['index'], 'method': 'POST' if upload else 'GET', 'path': self.path, 'body': json.loads(body) if body else None, 'headers': {key.lower(): value for key, value in self.headers.items() if key.lower() in ('user-agent', 'cf-access-client-id', 'cf-access-client-secret', 'content-type', 'accept', 'accept-encoding')}})
      cookie = SimpleCookie(); cookie.load(self.headers.get('Cookie', '')); outbound[-1]['cookies'] = sorted((name, value.value) for name, value in cookie.items()); outbound[-1]['cookie_raw'] = self.headers.get('Cookie', '')
      request_started.set()
      if mode in ('redirect', 'redirect_loop', 'redirect307', 'cookie_redirect') and (mode == 'redirect_loop' or not self.path.startswith('/owned-redirect')):
        self.send_response(307 if mode == 'redirect307' else 302); self.send_header('Location', '/owned-redirect'); self.send_header('Content-Length', '0'); self.send_header('Connection', 'close')
        if mode == 'cookie_redirect': self.send_header('Set-Cookie', 'owned_redirect=redirect; Path=/')
        self.end_headers(); return
      status = 503 if mode == 'failure' or mode == 'retry' and upload and counters[key] < 3 else 200; data = json.dumps(EMPTY).encode(); encoding = ''; charset = 'utf-8'
      if mode == 'hold': started.set(); gate.wait(timeout=8)
      if mode == 'timeout': time.sleep(1.2)
      if mode == 'rounding': time.sleep(5.35)
      if mode == 'nonobject' and not upload: data = b'[]'
      if mode == 'invalid_json' and not upload: data = b'{'
      if mode == 'invalid_utf8' and not upload: data = b'\xff'
      if mode == 'empty' and not upload: data = b''
      if mode == 'latin1' and not upload: data = json.dumps(dict(EMPTY, car_key='owned-é\x80'), ensure_ascii=False).encode('latin1'); charset = 'latin1'
      if mode == 'gzip': data = gzip.compress(data); encoding = 'gzip'
      if mode == 'br': data = brotli.compress(data); encoding = 'br'
      if mode in ('deflate', 'raw_deflate'): data = zlib.compress(data) if mode == 'deflate' else zlib.compress(data)[2:-4]; encoding = 'deflate'
      if mode == 'status204': status = 204; data = b''
      self.send_response(status); self.send_header('Content-Type', f'application/json; charset={charset}'); self.send_header('Content-Length', str(len(data))); self.send_header('Connection', 'close')
      if mode == 'cookie_seed': self.send_header('Set-Cookie', 'owned_session=persist; Path=/')
      if encoding: self.send_header('Content-Encoding', encoding)
      self.end_headers()
      try:
        self.wfile.write(data)
        if not upload: download_finished.set()
      except (BrokenPipeError, ConnectionResetError) as error: recipient_errors.append({'index': active['index'], 'error': type(error).__name__})
    def do_GET(self): self.exchange(False)
    def do_POST(self): self.exchange(True)
  recipient = ThreadingHTTPServer(('127.0.0.1', 0), Recipient); actor = threading.Thread(target=recipient.serve_forever, daemon=True); actor.start(); endpoint = f'http://127.0.0.1:{recipient.server_port}'
  owned_env = {'CARROT_PARAM_VALUE_URL': endpoint, 'CARROT_PARAM_VALUE_CF_ID': CF_ID, 'CARROT_PARAM_VALUE_CF_SECRET': CF_SECRET, 'CARROT_PARAM_VALUE_TIMEOUT_S': '1', 'CARROT_PARAM_VALUE_RETRY_COUNT': '3', 'CARROT_PARAM_VALUE_RETRY_DELAY_S': '1', 'CARROT_PARAM_VALUE_REFRESH_MIN_S': '60'}
  for key in list(os.environ):
    if key.startswith('CARROT_PARAM_VALUE_'): del os.environ[key]
  os.environ.update(owned_env)
  native = None; runner = None; stderr = (args.output / 'native.stderr').open('w'); exits = []; shutdowns = []
  try:
    phases = ('cookie-domain', 'cookie-ip') if args.cookies_only else ('rounding',) if args.rounding_only else ('composed', 'composed-cancel', 'composed-stall') if args.composed_only else ('normal',) if args.compression_only else ('normal', 'no-session', 'params-unavailable')
    for phase in phases:
      active['phase'] = phase
      os.environ['CARROT_PARAM_VALUE_URL'] = f'http://localhost:{recipient.server_port}' if phase == 'cookie-domain' else endpoint
      os.environ['CARROT_PARAM_VALUE_TIMEOUT_S'] = '5' if phase == 'rounding' else '1'
      for store in stores: seed(store, SEED)
      state_dirs = [args.output / (phase + '-' + side) for side in ('original', 'native')]
      for state in state_dirs:
        state.mkdir(exist_ok=True); (state / 'settings.json').write_text(json.dumps(CATALOG))
        if args.composed_only:
          (state / 'owned-web').mkdir(exist_ok=True); (state / 'owned-assets').mkdir(exist_ok=True); (state / 'owned-web/owned-health.txt').write_text('owned-health\n')
      settings.settings_cache.update(path=str(state_dirs[0] / 'settings.json'), mtime=0, data=None); source.HAS_PARAMS = phase != 'params-unavailable'; source._popular_values_memory = None; source._popular_refresh_last_at = 0.; source._popular_refresh_task = None
      async with aiohttp.ClientSession() as session:
        app = web.Application(); app['http'] = None if phase == 'no-session' else session; feature.register(app)
        async def fixture_wait(request):
          if source._popular_refresh_task is not None: await source._popular_refresh_task
          return web.json_response(source.read_popular_values_memory())
        async def fixture_seed(request): source._popular_values_memory = await request.json(); return web.json_response(source.read_popular_values_memory())
        async def fixture_schedule(request):
          data = await request.json(); before = source._popular_refresh_task; source.time = SimpleNamespace(time=lambda: data['now']); source.schedule_popular_value_refresh(app, data['interval']); source.time = SimpleNamespace(time=lambda: 1000.); return web.json_response(source._popular_refresh_task is not before)
        async def fixture_boot(request):
          task = source.start_popular_value_upload(app)
          if task is not None: await task
          return web.json_response(source.read_popular_values_memory())
        app.router.add_get('/__fixture/wait', fixture_wait); app.router.add_post('/__fixture/seed', fixture_seed); app.router.add_post('/__fixture/schedule', fixture_schedule); app.router.add_get('/__fixture/boot', fixture_boot)
        runner = web.AppRunner(app); await runner.setup(); site = web.TCPSite(runner, '127.0.0.1', 0); await site.start(); source_port = site._server.sockets[0].getsockname()[1]
        native_port = None
        if args.binary:
          if args.composed_only: active.update(side='native', index=-1, mode='hold' if phase == 'composed-stall' else 'failure' if phase == 'composed-cancel' else 'success'); request_started.clear(); download_finished.clear(); gate.clear() if phase == 'composed-stall' else gate.set()
          native = subprocess.Popen([str(args.binary.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True); assert native.stdin and native.stdout
          native.stdin.write(json.dumps({'http': True, 'root': str(roots[1].resolve()), 'state': str(state_dirs[1].resolve()), 'no_session': phase == 'no-session', 'unavailable': phase == 'params-unavailable', 'composed': args.composed_only}) + '\n'); native.stdin.flush(); native_port = json.loads(native.stdout.readline())['port']
          if args.composed_only:
            event = request_started if phase in ('composed-cancel', 'composed-stall') else download_finished
            assert await anyio.to_thread.run_sync(lambda: event.wait(timeout=8)), 'owned startup request absent'
        inputs = [('GET', BASE, 'success', None), ('GET', '/__fixture/wait', 'success', None), ('GET', BASE + '/detail?name=OwnedPopularInt&name=missing', 'success', None), ('HEAD', BASE + '/detail?name=missing', 'success', None), ('POST', BASE + '/refresh', 'success', None), ('GET', BASE + '/detail', 'success', None), ('GET', '/__fixture/boot', 'success', None), ('POST', BASE, 'success', None), ('GET', BASE + '/refresh', 'success', None)]
        if args.compression_only: inputs = [('POST', BASE + '/refresh', mode, None) for mode in ('gzip', 'br')]
        elif args.cookies_only: inputs = [('GET', BASE, 'cookie_seed', None), ('GET', '/__fixture/wait', 'cookie_seed', None), ('POST', BASE + '/refresh', 'cookie_redirect', None), ('POST', BASE + '/refresh', 'success', None)]
        elif args.rounding_only: inputs = [('GET', BASE, 'rounding', None), ('GET', '/__fixture/wait', 'rounding', None)]
        elif args.composed_only: inputs = [('GET', '/owned-health.txt', 'failure', None)] if phase in ('composed-cancel', 'composed-stall') else [('GET', BASE + '/detail?name=OwnedPopularInt', 'success', None), ('POST', BASE + '/refresh', 'success', None), ('GET', BASE + '/detail?name=OwnedPopularInt', 'success', None), ('POST', BASE + '/refresh', 'failure', None), ('GET', '/owned-health.txt', 'success', None), ('POST', BASE + '/refresh', 'success', None), ('GET', '/owned-health.txt', 'success', None)]
        elif phase == 'normal':
          inputs += [('POST', BASE + '/refresh', mode, None) for mode in ('retry', 'failure', 'nonobject', 'invalid_json', 'invalid_utf8', 'empty', 'gzip', 'br', 'deflate', 'raw_deflate', 'redirect', 'redirect_loop', 'redirect307', 'status204', 'latin1', 'success')]
          inputs += [('POST', '/__fixture/schedule', 'hold', {'now': 1060., 'interval': 60.}), ('POST', '/__fixture/schedule', 'hold', {'now': 1120., 'interval': 60.}), ('GET', '/__fixture/wait', 'success', None), ('POST', '/__fixture/schedule', 'success', {'now': 1119.9, 'interval': 60.}), ('POST', '/__fixture/schedule', 'success', {'now': 1120., 'interval': 60.}), ('GET', '/__fixture/wait', 'success', None), ('POST', BASE + '/refresh', 'timeout', None), ('POST', BASE + '/refresh', 'success', None)]
          inputs += [('GET', BASE + '/detail?name=OwnedPopularInt', 'car_change', None), ('POST', BASE + '/refresh', 'car_recover', None), ('GET', BASE + '/detail?name=OwnedPopularInt', 'catalog_change', None), ('POST', BASE + '/refresh', 'catalog_missing', None), ('POST', BASE + '/refresh', 'catalog_recover', None), ('POST', BASE + '/refresh', 'empty_car', None), ('POST', BASE + '/refresh', 'car_recover', None)]
        phase_rows = {side: [] for side in ('original', 'native')}; phase_outbound = {side: [] for side in ('original', 'native')}
        for side, port in (('original', source_port), ('native', native_port)):
          if args.composed_only and side == 'original': continue
          if port is None: continue
          phase_before = len(outbound)
          if args.rounding_only:
            await anyio.sleep((0.1 - time.monotonic() % 1.) % 1.)
          for index, (method, path, mode, data) in enumerate(inputs):
            store = stores[0 if side == 'original' else 1]; settings_path = state_dirs[0 if side == 'original' else 1] / 'settings.json'
            if mode in ('car_change', 'empty_car', 'car_recover'): seed(store, {'CarSelected3': 'Owned changed car' if mode == 'car_change' else '' if mode == 'empty_car' else SEED['CarSelected3']})
            if mode == 'catalog_change':
              changed = copy.deepcopy(CATALOG); changed['params'][2]['unit'] = 'owned changed unit'; modified = settings_path.stat().st_mtime + 2.; settings_path.write_text(json.dumps(changed)); os.utime(settings_path, (modified, modified))
            if mode == 'catalog_missing': settings_path.rename(settings_path.with_suffix('.missing'))
            if mode == 'catalog_recover': settings_path.with_suffix('.missing').rename(settings_path)
            active.update(side=side, index=index, mode=mode); before = len(outbound); gate.clear() if mode == 'hold' or phase == 'composed-stall' else gate.set(); started.clear() if mode == 'hold' and path == '/__fixture/schedule' and data['now'] == 1060 else None
            start = time.monotonic()
            async with session.request(method, f'http://127.0.0.1:{port}{path}', json=data if data is not None else None) as response:
              raw = await response.read(); row = {'status': response.status, 'body_hex': raw.hex(), 'headers': {key.lower(): value for key, value in response.headers.items() if key.lower() in ('content-type', 'content-length', 'allow')}}
            if mode == 'hold': await anyio.to_thread.run_sync(lambda: started.wait(timeout=5))
            calls = [dict(call, side='normalized') for call in outbound[before:]]; row['outbound'] = calls; phase_rows[side].append(row); observations.append({'kind': 'http', 'phase': phase, 'side': side, 'index': index, 'path': path, 'mode': mode, 'elapsed_seconds': round(time.monotonic() - start, 3), 'output': row})
          if phase != 'composed-stall': gate.set()
          phase_outbound[side] = [{key: value for key, value in call.items() if key not in ('side', 'index', 'cookie_raw')} for call in outbound[phase_before:]]
          if args.rounding_only: assert json.loads(bytes.fromhex(phase_rows[side][-1]['body_hex']))['source'] == 'remote'
        if args.composed_only:
          for index, row in enumerate(phase_rows['native']):
            assert row['status'] == 200
            if inputs[index][1] == '/owned-health.txt': assert bytes.fromhex(row['body_hex']) == b'owned-health\n'
            elif inputs[index][1].endswith('/refresh'): assert json.loads(bytes.fromhex(row['body_hex']))['uploaded'] == (inputs[index][2] == 'success')
            else: assert json.loads(bytes.fromhex(row['body_hex']))['detail'] == EMPTY['popular_values']['OwnedPopularInt']
          calls = [call for call in outbound if call['side'] == 'native' and call['phase'] == phase and call['index'] == -1]
          assert any(call['method'] == 'POST' for call in calls)
          if phase == 'composed': assert any(call['method'] == 'GET' for call in calls)
        elif args.binary:
          for index, (original, compiled) in enumerate(zip(phase_rows['original'], phase_rows['native'], strict=True)):
            if {key: value for key, value in original.items() if key != 'outbound'} != {key: value for key, value in compiled.items() if key != 'outbound'}: failures.append({'kind': 'http', 'phase': phase, 'index': index, 'input': inputs[index], 'original': original, 'native': compiled})
          if phase_outbound['original'] != phase_outbound['native']: failures.append({'kind': 'outbound', 'phase': phase, 'original': phase_outbound['original'], 'native': phase_outbound['native']})
          if args.cookies_only:
            for calls in phase_outbound.values():
              if phase == 'cookie-ip': assert all(not call['cookies'] for call in calls)
              else: assert ['owned_redirect', 'redirect'] in [list(cookie) for cookie in calls[-1]['cookies']] and ['owned_session', 'persist'] in [list(cookie) for cookie in calls[-1]['cookies']]
        await runner.cleanup(); runner = None
        if native:
          assert native.stdin; stop_started = time.monotonic(); native.stdin.write('stop\n'); native.stdin.flush(); native.stdin.close(); code = native.wait(timeout=10); exits.append(code); shutdowns.append({'phase': phase, 'exit': code, 'elapsed_seconds': time.monotonic() - stop_started}); native = None; gate.set()
  finally:
    gate.set()
    if runner: await runner.cleanup()
    if native: native.terminate(); native.wait(timeout=10)
    recipient.shutdown(); recipient.server_close(); actor.join(); stderr.close()
    for key in list(os.environ):
      if key.startswith('CARROT_PARAM_VALUE_'): del os.environ[key]
    os.environ.update(old_env)
    save(args.output / 'observations.json', observations); save(args.output / 'failures.json', failures); save(args.output / 'outbound.json', outbound); save(args.output / 'recipient-errors.json', recipient_errors); save(args.output / 'shutdowns.json', shutdowns)
  result = {'passed': bool(args.binary) and not failures and all(code == 0 for code in exits), 'policy_cases': len(policy_inputs), 'http_observations': sum(row['kind'] == 'http' for row in observations), 'failures': len(failures), 'exits': exits, 'comparison': 'owned Application startup/upload/download, actual popular routes, failure isolation and startup-retry cancellation' if args.composed_only else 'unchanged source policy and actual Params/HTTP cache, payload, retry, throttle, in-flight, error and recovery boundaries; owned loopback recipients only'}
  save(args.output / 'result.json', result); print(json.dumps(result)); assert not failures and all(code == 0 for code in exits)

if __name__ == '__main__': anyio.run(main, backend='asyncio')

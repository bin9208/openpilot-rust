import argparse
import asyncio
from contextlib import redirect_stderr
from copy import deepcopy
from dataclasses import dataclass
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import ModuleType
from typing import TypeAlias

from aiohttp import ClientSession, web
from original_params_binding import load

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']
TIMESTAMP = 1700000123
PRESET_KEYS = ('HyundaiCameraSCC', 'SpeedFromPCM', 'DisableDM', 'EnableRadarTracks',
               'EnableCornerRadar', 'AutoCruiseControl', 'AutoEngage')
OBSERVED_KEYS = (*PRESET_KEYS, 'CarSelected3', 'CarName', 'CanfdHDA2',
                 'LongitudinalPersonalityMax', 'CompletedTrainingVersion', 'DongleId', 'CalibrationParams',
                 'IsMetric', 'UptimeOnroad', 'InstallDate', 'LiveParameters', 'CarParamsPersistent')
MARKERS = ('web_settings.json', 'setting_profiles.json', 'setting_favorites.json', 'youtube_live.json')
CATALOG: dict[str, Json] = {'params': [
  {'name': name, 'min': 0, 'max': 4, 'default': 99,
   'hidden_brands': ['toyota']} for name in PRESET_KEYS
] + [{'name': 'NoRegisteredIntroSetting', 'default': 7}, {'name': 'CarName', 'default': 'catalog-only'}]}


@dataclass(frozen=True)
class Scenario:
  name: str
  fixture: str = ''
  path: str = '/api/intro/state'
  method: str = 'GET'
  body: str = ''
  keep: bool = False
  status: int = 200
  reason: str = ''


def cases() -> list[Scenario]:
  complete = '/api/intro/complete'
  reset = '/api/intro/reset'
  preset = '/api/intro/apply_preset'
  rows = [
    Scenario('manager-defaults', reason='fresh_install'),
    Scenario('generic-openpilot-state', 'generic', reason='fresh_install'),
    Scenario('chosen-car-first', 'car', reason='car_already_selected'),
    Scenario('chosen-car-next-read', keep=True, reason='existing_install'),
    Scenario('mock-car', 'mock-car', reason='fresh_install'),
    Scenario('blank-car', 'blank-car', reason='fresh_install'),
    Scenario('dash-car', 'dash-car', reason='fresh_install'),
    Scenario('fingerprinted-car', 'fingerprint', reason='car_fingerprinted'),
    Scenario('mock-fingerprint', 'mock-fingerprint', reason='fresh_install'),
    Scenario('changed-setting-before-fingerprint', 'changed-and-fingerprint', reason='setting_changed:SpeedFromPCM'),
    Scenario('changed-setting-next-read', keep=True, reason='existing_install'),
    Scenario('ordered-unfiltered-settings', 'ordered-settings', reason='setting_changed:HyundaiCameraSCC'),
    Scenario('missing-defaulted-value', 'missing-setting', reason='setting_changed:SpeedFromPCM'),
    Scenario('malformed-safe-value', 'malformed-setting', reason='setting_changed:SpeedFromPCM'),
    Scenario('prefix-is-not-safe-get', 'prefix-setting', reason='setting_changed:SpeedFromPCM'),
    Scenario('maximum-gap-prefix', 'gap-prefix', reason='fresh_install'),
    Scenario('safe-typed-defaults', 'typed-defaults', reason='fresh_install'),
    Scenario('safe-bool-exact-byte', 'typed-bool', reason='setting_changed:IsMetric'),
    Scenario('safe-float-full-parse', 'typed-float-underscore', reason='fresh_install'),
    Scenario('safe-float-malformed', 'typed-float-malformed', reason='setting_changed:UptimeOnroad'),
    Scenario('safe-string-invalid-utf8', 'invalid-utf8-car', reason='fresh_install'),
    Scenario('catalog-missing', 'catalog-missing', reason='fresh_install'),
    Scenario('catalog-malformed', 'catalog-malformed', reason='fresh_install'),
    Scenario('web-marker-priority', 'all-markers-and-car', reason='web_settings_exists'),
  ]
  rows.extend(Scenario('marker-' + name, 'marker-' + name, reason=reason)
              for name, reason in zip(MARKERS, ('web_settings_exists', 'setting_profiles_exists',
                                               'setting_favorites_exists', 'youtube_live_exists')))
  rows.extend([
    Scenario('marker-directory', 'marker-dir', reason='fresh_install'),
    Scenario('state-already-completed', 'completed', reason='saved_reason'),
    Scenario('state-completed-blank-reason', 'completed-empty', reason='already_completed'),
    Scenario('state-truthiness-coercions', 'state-coercions', reason="['reason', 3]"),
    Scenario('state-false-version', 'false-version', reason='fresh_install'),
    Scenario('state-invalid-version', 'invalid-version', status=500),
    Scenario('state-corrupt-json', 'corrupt-state', reason='fresh_install'),
    Scenario('state-nonobject', 'nonobject-state', reason='fresh_install'),
    Scenario('state-invalid-utf8', 'invalid-utf8-state', reason='fresh_install'),
    Scenario('complete-default', path=complete, method='POST', body='{}'),
    Scenario('complete-followup', keep=True, reason='user_finished'),
    Scenario('complete-invalid-json', path=complete, method='POST', body='{bad'),
    Scenario('complete-empty-list', path=complete, method='POST', body='[]'),
    Scenario('complete-true-list', path=complete, method='POST', body='[1]', status=500),
    Scenario('complete-reason-python-string', path=complete, method='POST', body='{"reason": [1, true]}'),
    Scenario('complete-unicode-truncation', path=complete, method='POST', body=json.dumps({'reason': '한🚗' * 40})),
    Scenario('complete-surrogate-write-failure', path=complete, method='POST', body='{"reason":"before\\ud800after"}'),
    Scenario('surrogate-write-next-read', keep=True, reason='fresh_install'),
    Scenario('complete-state-path-blocked', 'state-path-file', complete, 'POST', '{}'),
    Scenario('complete-replace-blocked', 'intro-dir', complete, 'POST', '{}'),
    Scenario('reset-existing-completion', 'completed', reset, 'POST', '{bad', reason='fresh_install'),
    Scenario('reset-no-state', path=reset, method='POST', reason='fresh_install'),
    Scenario('reset-existing-install', 'car', reset, 'POST', reason='car_already_selected'),
    Scenario('reset-directory-error', 'intro-dir', reset, 'POST', status=500),
    Scenario('preset-radar', path=preset, method='POST', body='{"preset":"radar_long"}'),
    Scenario('preset-camera', path=preset, method='POST', body='{"preset":"camera_long"}'),
    Scenario('preset-stock', path=preset, method='POST', body='{"preset":"stock"}'),
    Scenario('preset-python-whitespace', path=preset, method='POST', body='{"preset":"\\u001c radar_long \\u001f"}'),
    Scenario('preset-unknown', path=preset, method='POST', body='{"preset":"future"}', status=400),
    Scenario('preset-false-name', path=preset, method='POST', body='{"preset":false}', status=400),
    Scenario('preset-surrogate-name', path=preset, method='POST', body='{"preset":"\\ud800"}', status=400),
    Scenario('preset-invalid-json', path=preset, method='POST', body='{bad', status=400),
    Scenario('preset-true-nonobject', path=preset, method='POST', body='[1]', status=500),
    Scenario('preset-empty-nonobject', path=preset, method='POST', body='[]', status=400),
    Scenario('preset-fractional-clamp', 'fractional-clamp', preset, 'POST', '{"preset":"radar_long"}'),
    Scenario('preset-integer-clamp', 'integer-clamp', preset, 'POST', '{"preset":"stock"}'),
    Scenario('preset-malformed-catalog', 'catalog-malformed', preset, 'POST', '{"preset":"camera_long"}'),
    Scenario('preset-partial-conversion-failure', 'overflow-clamp', preset, 'POST', '{"preset":"stock"}', status=500),
    Scenario('preset-registered-write-blocked', 'blocked-param', preset, 'POST', '{"preset":"radar_long"}'),
    Scenario('state-head', method='HEAD'),
    Scenario('state-post-rejected', method='POST', status=405),
    Scenario('complete-get-rejected', path=complete, status=405),
  ])
  return rows


def remove(path: Path) -> None:
  if path.is_dir() and not path.is_symlink():
    shutil.rmtree(path)
  elif path.exists() or path.is_symlink():
    path.unlink()


class Fixture:
  def __init__(self, root: Path, params_class):
    self.root = root
    self.data = root / 'data'
    self.state = self.data / 'state'
    self.settings = root / 'settings.json'
    self.params_root = root / 'params'
    self.state.mkdir(parents=True)
    self.params = params_class(str(self.params_root))
    self.param_dir = Path(self.params.get_param_path())
    for key in (*PRESET_KEYS, 'CarSelected3', 'LongitudinalPersonalityMax', 'CompletedTrainingVersion',
                'IsMetric', 'UptimeOnroad'):
      default = self.params.get_default_value(key)
      if default is not None:
        self.params.put(key, default)
    self.params.put_int('CanfdHDA2', 1)
    self.defaults = {key: (self.param_dir / key).read_bytes() for key in OBSERVED_KEYS
                     if (self.param_dir / key).is_file()}
    self.serial = 0

  def catalog(self, value: Json) -> None:
    self.serial += 1
    self.settings.write_text(json.dumps(value), encoding='utf-8')
    os.utime(self.settings, (TIMESTAMP + self.serial, TIMESTAMP + self.serial))

  def put_raw(self, key: str, raw: bytes) -> None:
    path = self.param_dir / key
    remove(path)
    path.write_bytes(raw)

  def prepare(self, scenario: Scenario) -> None:
    if scenario.keep:
      return
    remove(self.state)
    self.state.mkdir(parents=True)
    for key in OBSERVED_KEYS:
      path = self.param_dir / key
      remove(path)
      if key in self.defaults:
        path.write_bytes(self.defaults[key])
    self.catalog(deepcopy(CATALOG))
    intro = self.state / 'intro.json'
    match scenario.fixture:
      case '':
        pass
      case 'generic':
        self.params.put('DongleId', 'owned-test-device')
        self.params.put('CalibrationParams', b'generic-calibration-fixture')
      case 'car':
        self.put_raw('CarSelected3', b'HYUNDAI IONIQ 5')
      case 'mock-car':
        self.put_raw('CarSelected3', b'  pre-MoCk-post  ')
      case 'blank-car':
        self.put_raw('CarSelected3', b'\x1c \t\x1f')
      case 'dash-car':
        self.put_raw('CarSelected3', b' - ')
      case 'fingerprint':
        self.put_raw('CarName', b'HYUNDAI IONIQ 5')
      case 'mock-fingerprint':
        self.put_raw('CarName', b' MoCk car ')
      case 'changed-and-fingerprint':
        self.put_raw('SpeedFromPCM', b'4')
        self.put_raw('CarName', b'TOYOTA')
      case 'ordered-settings':
        self.put_raw('HyundaiCameraSCC', b'4')
        self.put_raw('SpeedFromPCM', b'4')
      case 'missing-setting':
        remove(self.param_dir / 'SpeedFromPCM')
      case 'malformed-setting':
        self.put_raw('SpeedFromPCM', b'bad')
      case 'prefix-setting':
        self.put_raw('SpeedFromPCM', b'2tail')
      case 'gap-prefix':
        self.put_raw('LongitudinalPersonalityMax', b'4tail')
      case name if name.startswith('typed-'):
        catalog = deepcopy(CATALOG)
        catalog['params'].extend({'name': key, 'default': 'catalog-only'} for key in
                                 ('IsMetric', 'UptimeOnroad', 'InstallDate', 'LiveParameters', 'CarParamsPersistent'))
        self.catalog(catalog)
        self.put_raw('InstallDate', b'2026-10-08T01:23:45+09:00')
        self.put_raw('LiveParameters', b'{"value":1}')
        self.put_raw('CarParamsPersistent', b'\xff\x00')
        if name == 'typed-bool':
          self.put_raw('IsMetric', b'true')
        elif name == 'typed-float-underscore':
          self.put_raw('UptimeOnroad', b'0_0.0')
        elif name == 'typed-float-malformed':
          self.put_raw('UptimeOnroad', b'bad')
      case 'invalid-utf8-car':
        self.put_raw('CarSelected3', b'\xff')
      case 'catalog-missing':
        self.settings.unlink()
      case 'catalog-malformed':
        self.settings.write_text('{bad')
      case 'all-markers-and-car':
        for name in MARKERS:
          (self.state / name).write_bytes(b'')
        self.put_raw('CarSelected3', b'real-car')
      case name if name.startswith('marker-') and name[7:] in MARKERS:
        (self.state / name[7:]).write_bytes(b'')
      case 'marker-dir':
        (self.state / 'web_settings.json').mkdir()
      case 'completed':
        intro.write_text(json.dumps({'completed': True, 'reason': 'saved_reason', 'completedAt': 17, 'version': 7}))
      case 'completed-empty':
        intro.write_text('{"completed":true,"reason":""}')
      case 'state-coercions':
        intro.write_text('{"version":"2","completed":"yes","completedAt":[1],"reason":["reason",3]}')
      case 'false-version':
        intro.write_text('{"version":0,"completedAt":[],"reason":false}')
      case 'invalid-version':
        intro.write_text('{"version":"invalid"}')
      case 'corrupt-state':
        intro.write_text('{bad')
      case 'nonobject-state':
        intro.write_text('[1]')
      case 'invalid-utf8-state':
        intro.write_bytes(b'\xff')
      case 'state-path-file':
        remove(self.state)
        self.state.write_bytes(b'blocked')
      case 'intro-dir':
        intro.mkdir()
      case 'fractional-clamp' | 'integer-clamp' | 'overflow-clamp':
        catalog = deepcopy(CATALOG)
        params = catalog['params']
        if scenario.fixture == 'fractional-clamp':
          params[0].update(min=1.5, max=1.5, default=0.0)
          params[1].update(min=2.5, max=2.5, default=0.0)
        elif scenario.fixture == 'integer-clamp':
          params[0].update(min=3, max=3, default=0)
          params[6].update(min=0, max=1, default=0)
        else:
          params[2].update(min=2**40, max=2**40, default=0)
        self.catalog(catalog)
      case 'blocked-param':
        path = self.param_dir / 'DisableDM'
        remove(path)
        path.mkdir()
      case other:
        raise ValueError('unknown fixture: ' + other)

  def snapshot(self) -> dict:
    paths = {'state/' + name: self.state / name for name in
             (*MARKERS, 'intro.json', 'intro.json.tmp', 'param_changes.jsonl', 'param_changes_baseline.json')}
    paths.update({'params/' + name: self.param_dir / name for name in OBSERVED_KEYS})
    result = {}
    for name, path in paths.items():
      if path.is_file():
        result[name] = {'hex': path.read_bytes().hex()}
      elif path.is_dir():
        result[name] = {'directory': True}
      else:
        result[name] = None
    return result


async def compare(binary: Path, output: Path, binding: Path) -> None:
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-intro-') as temporary:
    root = Path(temporary)
    module, _ = load(binding, f'ipc://{root}/source-log.sock', output / 'binding-logs')
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.selfdrive.carrot.server.features.intro import routes, state
    from openpilot.selfdrive.carrot.server.services import params, settings
    fixtures = {side: Fixture(root / side, module.Params) for side in ('original', 'native')}
    source = fixtures['original']
    state.CARROT_STATE_DIR = str(source.state)
    state.CARROT_INTRO_STATE_PATH = str(source.state / 'intro.json')
    for attribute, name in zip(('CARROT_WEB_SETTINGS_PATH', 'CARROT_SETTING_PROFILES_PATH',
                                'CARROT_SETTING_FAVORITES_PATH', 'CARROT_YOUTUBE_LIVE_STATE_PATH'), MARKERS):
      setattr(state, attribute, str(source.state / name))
    state.time.time = lambda: TIMESTAMP
    params.Params = state.Params = lambda: source.params
    settings.settings_cache.update(path=str(source.settings), mtime=0, data=None)
    (output / 'fixture-defaults.json').write_text(json.dumps({key: raw.decode('utf-8') for key, raw in source.defaults.items()}, indent=2))
    app = web.Application()
    routes.register(app)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    source_port = site._server.sockets[0].getsockname()[1]
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             text=True, bufsize=1)
    if child.stdin is None or child.stdout is None or child.stderr is None:
      raise RuntimeError('fixture pipes are required')
    native = fixtures['native']
    child.stdin.write(json.dumps({'repository': str(native.root), 'data': str(native.data),
                                 'settings': str(native.settings), 'params': str(native.params_root),
                                 'has_params': True, 'timestamp': TIMESTAMP}) + '\n')
    child.stdin.flush()
    rows = {'original': [], 'native': []}
    differences = []
    try:
      line = await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10)
      native_port = json.loads(line)['port']
      with (output / 'original.stderr').open('w') as errors, redirect_stderr(errors):
        async with ClientSession() as session:
          for index, scenario in enumerate(cases()):
            for side, port in (('original', source_port), ('native', native_port)):
              fixture = fixtures[side]
              fixture.prepare(scenario)
              async with session.request(scenario.method, f'http://127.0.0.1:{port}{scenario.path}',
                                         data=scenario.body.encode('utf-8'), headers={'Content-Type': 'application/json'}) as response:
                body = await response.read()
                raw = {'method': scenario.method, 'path': scenario.path, 'request_body': scenario.body,
                       'status': response.status, 'headers': dict(response.headers), 'body_hex': body.hex()}
                (output / f'{index:03}-{side}-wire.json').write_text(json.dumps(raw, indent=2))
                body = body.replace(str(fixture.root).encode(), b'<fixture>')
                result = {'name': scenario.name, 'status': response.status,
                          'headers': {key.lower(): value for key, value in response.headers.items()
                                      if key.lower() in ('content-type', 'content-length', 'allow')},
                          'body': body.decode('utf-8'), 'files': fixture.snapshot()}
              if '<fixture>' in result['body']:
                result['headers'].pop('content-length', None)
              rows[side].append(result)
              (output / f'{index:03}-{side}.json').write_text(json.dumps(result, indent=2, ensure_ascii=True))
            if rows['original'][-1] != rows['native'][-1]:
              differences.append({'name': scenario.name, 'original': rows['original'][-1], 'native': rows['native'][-1]})
            if rows['original'][-1]['status'] != scenario.status:
              differences.append({'name': scenario.name, 'unexpected_source_status': rows['original'][-1]['status'], 'expected': scenario.status})
            if scenario.reason:
              original_body = json.loads(rows['original'][-1]['body'])
              if original_body['reason'] != scenario.reason:
                differences.append({'name': scenario.name, 'unexpected_source_reason': original_body['reason'], 'expected': scenario.reason})
            if scenario.name.startswith('preset-'):
              snapshot = rows['original'][-1]['files']
              if snapshot['state/param_changes.jsonl'] is not None:
                differences.append({'name': scenario.name, 'unexpected_immediate_history': snapshot['state/param_changes.jsonl']})
              if snapshot['params/CanfdHDA2'] != {'hex': source.defaults['CanfdHDA2'].hex()}:
                differences.append({'name': scenario.name, 'unexpected_hda_change': snapshot['params/CanfdHDA2']})
          params.HAS_PARAMS = state.HAS_PARAMS = routes.HAS_PARAMS = False
          child.stdin.write('stop\n')
          child.stdin.flush()
          code = await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
          (output / 'native.stderr').write_text(child.stderr.read())
          if code != 0:
            differences.append({'native_exit_code': code})
          absent = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    text=True, bufsize=1)
          if absent.stdin is None or absent.stdout is None or absent.stderr is None:
            raise RuntimeError('fixture pipes are required')
          absent.stdin.write(json.dumps({'repository': str(native.root), 'data': str(native.data),
                                        'settings': str(native.settings), 'params': str(native.params_root),
                                        'has_params': False, 'timestamp': TIMESTAMP}) + '\n')
          absent.stdin.flush()
          absent_port = json.loads(await asyncio.wait_for(asyncio.to_thread(absent.stdout.readline), 10))['port']
          absent_rows = []
          try:
            for scenario in (Scenario('params-absent-fresh', reason='fresh_install'),
                             Scenario('params-absent-invalid-body', path='/api/intro/apply_preset', method='POST', body='{bad', status=500)):
              pair = {}
              for side, port in (('original', source_port), ('native', absent_port)):
                fixtures[side].prepare(scenario)
                async with session.request(scenario.method, f'http://127.0.0.1:{port}{scenario.path}',
                                           data=scenario.body.encode(), headers={'Content-Type': 'application/json'}) as response:
                  pair[side] = {'status': response.status, 'body': (await response.read()).decode(),
                                'files': fixtures[side].snapshot()}
              absent_rows.append({'name': scenario.name, **pair})
              if pair['original'] != pair['native'] or pair['original']['status'] != scenario.status:
                differences.append({'name': scenario.name, **pair})
          finally:
            absent.stdin.write('stop\n')
            absent.stdin.flush()
            absent_code = await asyncio.wait_for(asyncio.to_thread(absent.wait), 10)
            (output / 'absent-native.stderr').write_text(absent.stderr.read())
            if absent_code != 0:
              differences.append({'absent_exit_code': absent_code})
          (output / 'absent-params.json').write_text(json.dumps(absent_rows, indent=2))
    finally:
      if child.poll() is None:
        child.stdin.write('stop\n')
        child.stdin.flush()
        try:
          await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
        except TimeoutError:
          child.kill()
          child.wait()
      await runner.cleanup()
    for side in rows:
      (output / (side + '.json')).write_text(json.dumps(rows[side], indent=2))
    report = {'scenarios': len(rows['original']), 'params_absent_scenarios': len(absent_rows),
              'differences': differences, 'pass': not differences}
    (output / 'result.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'scenarios': report['scenarios'], 'params_absent_scenarios': report['params_absent_scenarios'],
                      'differences': len(differences), 'pass': report['pass']}))
    if differences:
      raise SystemExit(1)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--binding', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  arguments = parser.parse_args()
  asyncio.run(compare(arguments.binary.resolve(), arguments.output, arguments.binding.resolve()))


if __name__ == '__main__':
  main()

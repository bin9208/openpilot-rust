import argparse
import asyncio
from dataclasses import dataclass
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from types import ModuleType, SimpleNamespace

from aiohttp import ClientSession, web
from original_params_binding import load

SHA = 'a' * 64
NOW = 1700000123.125
STATUS = '/api/egpu/model'
RESTART = STATUS + '/compile-restart'


@dataclass(frozen=True)
class Scenario:
  name: str
  method: str = 'GET'
  path: str = STATUS
  status: int = 200


def cases() -> list[Scenario]:
  rows = [Scenario(name) for name in (
    'never-connected', 'seen-no-model', 'active-ready', 'active-relative-url',
    'active-dot-filename', 'active-empty-https', 'active-triple-slash',
    'compiled-local', 'compiled-relative', 'pkl-not-local-compiled', 'compiled-installed',
    'installed-rejected', 'installed-invalid-runtime', 'wrong-model-size', 'invalid-previous',
    'invalid-active-size-bool', 'invalid-active-url', 'missing-model', 'malformed-state',
    'malformed-status', 'invalid-schema', 'schema-true', 'invalid-phase', 'status-null',
    'status-nonobject', 'invalid-status-utf8', 'same-status', 'different-status', 'no-status-identity',
    'progress-negative', 'progress-over', 'progress-zero', 'progress-round-low',
    'progress-round-high', 'progress-string', 'progress-float', 'status-pass-through', 'seen-byte-not-one',
  )]
  rows.extend(Scenario('phase-' + phase) for phase in (
    'checking', 'downloading', 'verifying', 'ready', 'waiting_for_ignition', 'compiling', 'compiled', 'error'))
  rows.extend(Scenario(name, status=500) for name in ('invalid-count-null', 'invalid-count-list', 'huge-count', 'state-directory'))
  rows.extend(Scenario(name, 'POST', RESTART, code) for name, code in (
    ('restart-never-connected', 404), ('restart-engaged', 409), ('restart-no-model', 409),
    ('restart-updating', 409), ('restart-compiled', 409), ('restart-no-usb', 409),
    ('restart-usb-slow', 409), ('restart-usb-boundary', 200), ('restart-usb-alternate', 200),
    ('restart-usb-float', 200), ('restart-usb-malformed-plus-valid', 200),
    ('restart-ready', 200), ('restart-repeat', 200), ('restart-error', 200),
    ('restart-waiting-old', 200), ('restart-waiting-zero', 200), ('restart-waiting-string', 200), ('restart-waiting-bad', 500),
    ('restart-status-directory', 500), ('restart-param-directory', 200), ('restart-recovery', 200),
    ('restart-model-removed-during-usb', 409)))
  rows.extend((Scenario('head', 'HEAD'), Scenario('post-status', 'POST', status=405),
               Scenario('get-restart', path=RESTART, status=405),
               Scenario('head-restart', 'HEAD', RESTART, 405),
               Scenario('options-restart', 'OPTIONS', RESTART, 405),
               Scenario('encoded-route', path='/api/egpu/%6dodel'),
               Scenario('gzip-request-no-compression')))
  return rows


def remove(path: Path) -> None:
  if path.is_dir() and not path.is_symlink():
    shutil.rmtree(path)
  elif path.exists() or path.is_symlink():
    path.unlink()


class Files:
  def __init__(self, root: Path, params_class):
    self.root = root
    self.params = params_class(str(root / 'params'))
    self.directory = Path(self.params.get_param_path())
    self.cache = root / 'cache'
    self.models = root / 'models'
    self.usb = root / 'usb'
    self.repository = root / 'repository'
    self.web = self.repository / 'openpilot/selfdrive/carrot/web'
    self.assets = self.repository / 'openpilot/selfdrive/assets'
    self.web.mkdir(parents=True)
    self.assets.mkdir(parents=True)
    self.settings = root / 'settings.json'
    self.settings.write_text('{"params":[]}')

  def setup(self, name: str) -> None:
    if name == 'restart-repeat':
      return
    for path in (self.cache, self.models, self.usb):
      remove(path)
      path.mkdir()
    for key in ('UsbGpuHardwareSeen', 'IsEngaged', 'DoReboot'):
      remove(self.directory / key)
    self.params.put_bool('UsbGpuHardwareSeen', name not in ('never-connected', 'restart-never-connected'))
    self.params.put_bool('IsEngaged', name == 'restart-engaged')
    if name == 'seen-byte-not-one':
      (self.directory / 'UsbGpuHardwareSeen').write_bytes(b'01')
    manifest = {'model_id': 'owned-model', 'filename': 'model.onnx', 'size': 10, 'sha256': SHA,
                'url': 'https://owned.invalid/model.onnx'}
    state = {'active': manifest, 'previous': None}
    if name in ('active-relative-url', 'compiled-relative'):
      manifest['url'] = 'model.onnx'
    if name == 'active-dot-filename':
      manifest['filename'] = '.onnx'
    if name == 'active-empty-https':
      manifest['url'] = 'https://'
    if name == 'active-triple-slash':
      manifest['url'] = 'https:///model.onnx'
    if name == 'invalid-active-url':
      manifest['url'] = 'http://owned.invalid/model.onnx'
    if name == 'invalid-active-size-bool':
      manifest['size'] = True
    if name == 'invalid-previous':
      state['previous'] = {'model_id': 'broken'}
    if name == 'pkl-not-local-compiled' or name.startswith('installed-') or name == 'compiled-installed':
      manifest['filename'] = 'model.pkl'
    model_name = Path(manifest['filename'])
    model = self.cache / f'{model_name.stem}-{SHA[:16]}{model_name.suffix}'
    model.write_bytes(b'ownedmodel' if name != 'wrong-model-size' else b'x')
    (self.cache / 'state.json').write_text(json.dumps(state))
    if name in ('seen-no-model', 'restart-no-model'):
      remove(self.cache / 'state.json')
    if name == 'missing-model':
      model.unlink()
    if name == 'malformed-state':
      (self.cache / 'state.json').write_text('{bad')
    if name == 'state-directory':
      remove(self.cache / 'state.json')
      (self.cache / 'state.json').mkdir()
    if name in ('compiled-local', 'compiled-relative', 'pkl-not-local-compiled', 'restart-compiled'):
      (self.models / f'big_driving_{SHA[:16]}_tinygrad.pkl.chunkmanifest').write_text('{}')
    if name == 'compiled-installed' or name.startswith('installed-'):
      installed = self.cache / 'precompiled' / SHA
      runtime = installed / ('runtime-' + 'b' * 16)
      (runtime / 'examples/openpilot').mkdir(parents=True)
      (runtime / 'tinygrad').mkdir()
      (runtime / 'examples/openpilot/compile_warp.py').write_text('')
      (runtime / 'tinygrad/__init__.py').write_text('')
      (installed / 'model.pkl').write_bytes(b'ownedmodel')
      catalog = {'protocol': 1, 'format': 'comma-generic-onnx', 'model_sha256': SHA, 'gpu_arch': 'gfx1200',
                 'frame_skip': 4, 'camera_resolutions': [[1928, 1208], [1344, 760]],
                 'catalog_url': 'https://owned.invalid/precompiled.json', 'runtime_directory': runtime.name,
                 'pickle': {'sha256': SHA, 'size': 10, 'url': 'model.pkl'},
                 'runtime': {'sha256': 'b' * 64, 'size': 10, 'url': 'runtime.tar'}}
      (installed / 'installed.json').write_text(json.dumps(catalog))
      if name == 'installed-rejected':
        (installed / 'rejected').write_text('')
      if name == 'installed-invalid-runtime':
        remove(runtime / 'tinygrad/__init__.py')
    value = {'schema_version': 1, 'state': 'ready', 'sha256': SHA, 'model_id': 'status-model'}
    needs_status = name.startswith(('phase-', 'progress-', 'invalid-count-', 'restart-waiting-')) or name in (
      'same-status', 'different-status', 'no-status-identity', 'schema-true', 'invalid-schema',
      'invalid-phase', 'status-pass-through', 'restart-error', 'restart-updating', 'huge-count')
    if name.startswith('phase-'):
      value['state'] = name.removeprefix('phase-')
    if name == 'restart-updating':
      value['state'] = 'downloading'
    if name == 'restart-error':
      value['state'] = 'error'
    if name.startswith('restart-waiting-'):
      value.update(state='waiting_for_ignition', started_at={'old': 12.5, 'zero': 0, 'string': '12.5', 'bad': 'not a timestamp'}[name.removeprefix('restart-waiting-')])
    if name == 'different-status':
      value['sha256'] = 'c' * 64
    if name == 'no-status-identity':
      del value['sha256']
      del value['model_id']
    if name == 'invalid-schema':
      value['schema_version'] = 2
    if name == 'schema-true':
      value['schema_version'] = True
    if name == 'invalid-phase':
      value['state'] = 'unknown'
    if name == 'status-pass-through':
      value.update(detail=['owned', 3], started_at='old', updated_at={'stamp': 2})
    progress = {'negative': (-1, 10), 'over': (12, 10), 'zero': (3, 0), 'round-low': (107, 2000),
                'round-high': (109, 2000), 'string': (' 1_0 ', '30'), 'float': (8.9, 30.8)}
    if name.startswith('progress-'):
      value['downloaded_bytes'], value['total_bytes'] = progress[name.removeprefix('progress-')]
    if name.startswith('invalid-count-'):
      value['downloaded_bytes'] = None if name.endswith('null') else []
    if name == 'huge-count':
      value['downloaded_bytes'] = 10**400
    if needs_status:
      (self.cache / 'status.json').write_text(json.dumps(value))
    for label, raw in (('malformed-status', b'{bad'), ('status-null', b'null'), ('status-nonobject', b'[]'),
                       ('invalid-status-utf8', b'\xff')):
      if name == label:
        (self.cache / 'status.json').write_bytes(raw)
    if name == 'restart-status-directory':
      (self.cache / 'status.json').mkdir()
    if name == 'restart-param-directory':
      (self.directory / 'DoReboot').mkdir()
    if name.startswith('restart-') and name != 'restart-no-usb':
      device = self.usb / 'owned-device'
      device.mkdir()
      (device / 'idVendor').write_text('3801' if name == 'restart-usb-alternate' else 'add1')
      (device / 'idProduct').write_text('0001')
      speed = {'restart-usb-slow': '4999.99', 'restart-usb-float': '5000.5'}.get(name, '5000')
      (device / 'speed').write_text(speed)
      if name == 'restart-model-removed-during-usb':
        (device / 'speed').unlink()
        os.mkfifo(device / 'speed')
      if name == 'restart-usb-malformed-plus-valid':
        (self.usb / 'bad-device').mkdir()
        (self.usb / 'bad-device/idVendor').write_text('not hex')

  def remove_model_when_usb_is_read(self) -> None:
    deadline = time.monotonic() + 5
    while True:
      try:
        descriptor = os.open(self.usb / 'owned-device/speed', os.O_WRONLY | os.O_NONBLOCK)
        break
      except OSError as error:
        if error.errno != 6 or time.monotonic() > deadline:
          raise
        time.sleep(0.005)
    try:
      (self.cache / f'model-{SHA[:16]}.onnx').unlink()
      os.write(descriptor, b'5000')
    finally:
      os.close(descriptor)

  def effects(self) -> dict:
    status = self.cache / 'status.json'
    reboot = self.directory / 'DoReboot'
    raw = status.read_bytes() if status.is_file() else None
    value = None
    if raw is not None:
      try:
        value = json.loads(raw)
      except (ValueError, UnicodeDecodeError):
        pass
    return {'status': value, 'status_hex': raw.hex() if raw is not None else None,
            'reboot_hex': reboot.read_bytes().hex() if reboot.is_file() else None,
            'temporary_status_files': sorted(p.name for p in self.cache.glob('.status-*'))}


async def compare(args) -> None:
  args.output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-egpu-') as temporary:
    root = Path(temporary)
    module, _ = load(args.binding, f'ipc://{root}/source-log.sock', args.output / 'binding-logs')
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.selfdrive.carrot.server.features import egpu_model as source
    from openpilot.selfdrive.modeld import big_model, big_model_status, helpers, precompiled_model
    original, native = (Files(root / side, module.Params) for side in ('original', 'native'))
    source.model_cache_dir = big_model.model_cache_dir = precompiled_model.model_cache_dir = lambda: original.cache
    source.Params = lambda: original.params
    helpers.MODELS_DIR = original.models
    helpers.Path = lambda value: original.usb if value == '/sys/bus/usb/devices' else Path(value)
    big_model_status.time = SimpleNamespace(time=lambda: NOW)
    app = web.Application()
    source.register(app)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    port = site._server.sockets[0].getsockname()[1]
    child = subprocess.Popen([str(args.binary.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, text=True, bufsize=1)
    config = {'repository': str(native.repository), 'data': str(native.root / 'data'), 'settings': str(native.settings),
              'params': str(native.root / 'params'), 'models': str(native.models), 'model_cache': str(native.cache),
              'usb_devices': str(native.usb), 'egpu_timestamp': NOW,
              'web': str(native.web), 'shared_assets': str(native.assets), 'training_assets': str(native.assets),
              'legacy_state': str(native.root / 'legacy'), 'unavailable': args.unavailable}
    child.stdin.write(json.dumps(config) + '\n')
    child.stdin.flush()
    rows, differences = [], []
    try:
      native_port = json.loads(await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10))['port']
      source.HAS_PARAMS = not args.unavailable
      scenarios = [Scenario('unavailable-status'), Scenario('unavailable-restart', 'POST', RESTART, 500)] if args.unavailable else cases()
      if args.first:
        scenarios = scenarios[:1]
      async with ClientSession(auto_decompress=False) as session:
        for scenario in scenarios:
          pair = {}
          for side, side_port, files in (('original', port, original), ('native', native_port, native)):
            files.setup(scenario.name)
            before = files.effects()
            mutation = asyncio.create_task(asyncio.to_thread(files.remove_model_when_usb_is_read)) if scenario.name == 'restart-model-removed-during-usb' else None
            async with session.request(scenario.method, f'http://127.0.0.1:{side_port}{scenario.path}',
                                       headers={'Accept-Encoding': 'gzip, deflate'}, data=b'{bad' if scenario.method == 'POST' else None) as response:
              body = await response.read()
              pair[side] = {'status': response.status, 'headers': {k.lower(): v for k, v in response.headers.items()
                                   if k.lower() not in ('date', 'server')}, 'body_hex': body.hex(),
                            'before': before, 'effects': files.effects()}
            if mutation is not None:
              await mutation
            (args.output / f'{len(rows):03}-{side}.json').write_text(json.dumps({'scenario': scenario.name, **pair[side]}, indent=2))
          row = {'name': scenario.name, **pair}
          rows.append(row)
          if pair['original'] != pair['native'] or pair['original']['status'] != scenario.status:
            differences.append(row)
        result = {'passed': not differences, 'observations': len(rows), 'differences': differences, 'rows': rows}
        (args.output / 'result.json').write_text(json.dumps(result, indent=2))
        print(json.dumps({'passed': result['passed'], 'observations': len(rows), 'differences': len(differences)}))
    finally:
      child.stdin.write('stop\n')
      child.stdin.flush()
      code = await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
      (args.output / 'native-stderr.log').write_text(child.stderr.read())
      (args.output / 'native-exit.json').write_text(json.dumps({'exit_code': code}))
      await runner.cleanup()
    if code != 0 or differences:
      raise AssertionError(f'eGPU comparison failed: exit={code}, differences={len(differences)}')


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--first', action='store_true')
  parser.add_argument('--unavailable', action='store_true')
  asyncio.run(compare(parser.parse_args()))

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


async def compare(binary: Path, output: Path, binding: Path):
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
    from openpilot.selfdrive.carrot.server.services import params as source_params
    from openpilot.selfdrive.carrot.server.services import settings as source_settings
    from openpilot.selfdrive.carrot.server.services import static_assets as source_assets
    if not source_params.HAS_PARAMS:
      raise RuntimeError('original native Params binding is required for this owned HTTP comparison')
    settings_file = root / 'settings.json'
    definition = {'apilot': 'owned', 'params': [
      {'name': 'CruiseGapLevels', 'group': 'Gap', 'min': 2, 'max': 4, 'default': 4,
       'options': {'ko': ['2', '3', '4']}},
      {'name': 'visible', 'group': 'Z'},
      {'name': 'hidden', 'group': 'Z', 'hidden_brands': ['hyundai']},
    ]}
    settings_file.write_text(json.dumps(definition))
    for name in ('web', 'shared_assets', 'training_assets', 'legacy_state', 'data'):
      (root / name).mkdir()
    (root / 'web/js').mkdir()
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
        for index, scenario in enumerate(scenarios):
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
          for side, port in (('original', source_port), ('native', native_port)):
            async with session.request(method, URL(f'http://127.0.0.1:{port}{path}', encoded=True), headers={'Accept-Encoding': coding}) as response:
              body = await response.read()
              (output / f'{index}-{side}.body').write_bytes(body)
              (output / f'{index}-{side}.headers.json').write_text(json.dumps(list(response.headers.items())))
              headers = {key: response.headers.get(key) for key in ('Content-Type', 'Content-Length', 'Cache-Control', 'Allow', 'Content-Encoding', 'Last-Modified', 'ETag', 'Accept-Ranges', 'Vary')}
              rows[side].append({'scenario': scenario, 'status': response.status, 'headers': headers, 'body_hex': body.hex()})
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
  args = parser.parse_args()
  asyncio.run(compare(args.binary, args.output, args.binding))


if __name__ == '__main__':
  main()

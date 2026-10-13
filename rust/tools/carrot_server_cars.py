import argparse
import asyncio
from dataclasses import dataclass
import importlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import ModuleType, SimpleNamespace

from aiohttp import ClientSession, web
from original_params_binding import load

BRANDS = ('hyundai', 'gm', 'toyota', 'mazda', 'ford', 'volkswagen', 'tesla')


@dataclass(frozen=True)
class Scenario:
  name: str
  fixture: str = ''
  method: str = 'GET'
  encoding: str = 'identity'
  status: int = 200
  path: str = '/api/cars'


def cases() -> list[Scenario]:
  return [Scenario('all-seven-brands'), Scenario('all-seven-brands-head', method='HEAD'),
          Scenario('owned-file-order-dedup', 'files'), Scenario('file-universal-newlines', 'newlines'),
          Scenario('file-python-whitespace', 'whitespace'), Scenario('file-utf8-ignore', 'invalid-content'),
          Scenario('filename-surrogateescape-order', 'invalid-filename'),
          Scenario('unreadable-directory-sources', 'directory'), Scenario('symlink-file', 'symlink'),
          Scenario('broken-and-cyclic-symlinks', 'broken-symlinks'),
          Scenario('missing-param-directory', 'missing'), Scenario('param-directory-is-file', 'not-directory'),
          Scenario('file-before-recovery', 'before-recovery'), Scenario('file-after-recovery', 'recovery'),
          Scenario('files-removed', 'missing'), Scenario('get-no-automatic-compression', encoding='gzip, deflate'),
          Scenario('post-method', method='POST', status=405), Scenario('options-method', method='OPTIONS', status=405),
          Scenario('unregistered-path', status=404, path='/api/not-cars')]


def remove(path: Path) -> None:
  if path.is_dir() and not path.is_symlink():
    shutil.rmtree(path)
  elif path.exists() or path.is_symlink():
    path.unlink()


def fixture(root: Path, directory: Path, name: str) -> None:
  if name == 'recovery':
    (directory / 'SupportedCarsA').write_bytes(b'Custom Recovered\nHyundai Azera 2022\n')
    return
  remove(directory)
  directory.mkdir()
  match name:
    case '':
      pass
    case 'files':
      (directory / 'SupportedCarsZ').write_bytes(b'Zeta Last\nAcme Roadster\nHyundai Azera 2022\n')
      (directory / 'SupportedCarsA').write_bytes(b'Acme   Roadster\nAcme Roadster\nNew First\nNoSpace\nMaker\tTabOnly\n')
      (directory / 'SupportedCars').write_bytes(b'Before Zero\n')
      (directory / 'OtherSupportedCars').write_bytes(b'Ignored Value\n')
    case 'newlines':
      (directory / 'SupportedCarsCR').write_bytes(b'Acme One\rAcme Two\r\nAcme Three\nAcme Final')
    case 'whitespace':
      (directory / 'SupportedCarsWS').write_text('\u001cAcme \t Roadster \u001f\n\u0085Beta\u00a0 Inner Space\u2003\nNoSpace\tOnly\nEmpty \t \n', encoding='utf-8')
    case 'invalid-content':
      (directory / 'SupportedCarsBad').write_bytes(b'Bad\xffMaker Mat\xffrix\nAcme Hatch\xc3\nUTF \xf0\x80badname\nValid \xef\xbf\xbdreplacement\n')
    case 'invalid-filename':
      raw = os.fsencode(directory)
      with open(raw + b'/SupportedCars-\xff', 'wb') as stream:
        stream.write(b'InvalidFilename Car\n')
      (directory / 'SupportedCars-\U00010000').write_bytes(b'SupplementaryFilename Car\n')
    case 'directory':
      (directory / 'SupportedCarsDirectory').mkdir()
      (directory / 'SupportedCarsA').write_bytes(b'Acme Readable\n')
    case 'symlink':
      target = root / 'outside-owned.txt'
      target.write_bytes(b'Linked OutsideOwned\n')
      (directory / 'SupportedCarsLink').symlink_to(target)
    case 'broken-symlinks':
      (directory / 'SupportedCarsBroken').symlink_to(root / 'missing-owned')
      (directory / 'SupportedCarsCycle').symlink_to('SupportedCarsCycle')
    case 'missing':
      directory.rmdir()
    case 'not-directory':
      directory.rmdir()
      directory.write_bytes(b'not a directory')
    case 'before-recovery':
      (directory / 'SupportedCarsA').write_bytes(b'Custom Old\n')
    case other:
      raise ValueError('unknown fixture: ' + other)


async def compare(binary: Path, binding: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=True)
  differences = []
  with tempfile.TemporaryDirectory(prefix='carrot-cars-') as temporary:
    root = Path(temporary)
    load(binding, f'ipc://{root}/source-log.sock', output / 'binding-logs')
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.selfdrive.carrot.server.features import cars as source
    source_root, native_root = root / 'original', root / 'native'
    source_root.mkdir()
    native_root.mkdir()
    source_dir, native_dir = source_root / 'params-d', native_root / 'params-d'
    source.SUPPORTED_CAR_GLOB = str(source_dir / 'SupportedCars*')
    app = web.Application()
    source.register(app)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    source_port = site._server.sockets[0].getsockname()[1]
    original_import = importlib.import_module
    real_names = {brand: [doc.name for platform in original_import('opendbc.car.' + brand + '.values').CAR
                          for doc in platform.config.car_docs] for brand in BRANDS}
    (output / 'source-seven-brands.json').write_text(json.dumps(real_names, ensure_ascii=True, indent=2))
    provider_names = [json.dumps(['  Provider  Double Space  ', 'Provider  Double Space', 'NoSpace', '\ud800 Car']),
                      '{bad', json.dumps([]), json.dumps(['Provider Second']),
                      json.dumps([123, None, False, 'Ford Custom']),
                      json.dumps(['Volkswagen Native']), json.dumps(['Tesla Native'])]
    async with ClientSession(auto_decompress=False) as session:
      rows = []
      for wave, catalogs in (('real', None), ('provider', provider_names)):
        if catalogs is not None:
          def provider(name: str, package: str | None = None):
            for index, brand in enumerate(BRANDS):
              if name == 'opendbc.car.' + brand + '.values':
                names = json.loads(catalogs[index])
                if not isinstance(names, list):
                  raise TypeError('fixture brand documentation is unavailable')
                return SimpleNamespace(CAR=[SimpleNamespace(config=SimpleNamespace(car_docs=[SimpleNamespace(name=value) for value in names]))])
            return original_import(name, package)
          source.importlib.import_module = provider
        child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        if child.stdin is None or child.stdout is None or child.stderr is None:
          raise RuntimeError('cars fixture pipes are required')
        settings = {'supported_cars': str(native_dir)}
        if catalogs is not None:
          settings['catalogs'] = catalogs
        child.stdin.write(json.dumps(settings) + '\n')
        child.stdin.flush()
        try:
          native_port = json.loads(await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10))['port']
          scenarios = cases() if catalogs is None else [Scenario('provider-brand-error-order'), Scenario('provider-files-before-brands', 'files')]
          for scenario in scenarios:
            pair = {}
            for side, port, owned_root, directory in (('original', source_port, source_root, source_dir),
                                                      ('native', native_port, native_root, native_dir)):
              fixture(owned_root, directory, scenario.fixture)
              async with session.request(scenario.method, f'http://127.0.0.1:{port}{scenario.path}',
                                         headers={'Accept-Encoding': scenario.encoding}) as response:
                body = await response.read()
                result = {'status': response.status, 'headers': {name.lower(): value for name, value in response.headers.items()
                                                                 if name.lower() in ('content-type', 'content-length', 'content-encoding', 'allow')},
                          'body_hex': body.hex(), 'body': body.decode('utf-8')}
              pair[side] = result
              (output / f'{len(rows):03}-{side}.json').write_text(json.dumps({'name': scenario.name, 'method': scenario.method, **result}, indent=2))
            rows.append({'name': scenario.name, **pair})
            if pair['original'] != pair['native'] or pair['original']['status'] != scenario.status:
              differences.append({'name': scenario.name, **pair})
            if scenario.name == 'filename-surrogateescape-order':
              names = json.loads(pair['original']['body'])['sources']
              if names != ['SupportedCars-\udcff', 'SupportedCars-\U00010000']:
                differences.append({'name': scenario.name, 'unexpected_source_names': names})
        finally:
          if child.poll() is None:
            child.stdin.write('stop\n')
            child.stdin.flush()
            try:
              code = await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
            except TimeoutError:
              child.kill()
              code = child.wait()
          else:
            code = child.returncode
          (output / (wave + '-native.stderr')).write_text(child.stderr.read())
          if code != 0:
            differences.append({'wave': wave, 'native_exit_code': code})
      source.importlib.import_module = original_import
      await runner.cleanup()
    (output / 'pairs.json').write_text(json.dumps(rows, indent=2))
    result = {'pass': not differences, 'http_pairs': len(rows), 'seven_brand_names': {brand: len(names) for brand, names in real_names.items()},
              'differences': differences, 'filesystem_scope': 'owned temporary directories; production Params path never opened',
              'filename_decoding': 'POSIX surrogateescape', 'file_content_decoding': 'UTF8 errors=ignore',
              'fixture_provider': 'declared seven brands with unavailable gm, empty toyota, whitespace/surrogate names and nonstring doc names'}
    (output / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps({'pass': result['pass'], 'http_pairs': result['http_pairs'], 'differences': len(differences)}))
    if differences:
      raise SystemExit(1)


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ('binary', 'binding', 'output'):
    parser.add_argument('--' + name, required=True, type=Path)
  args = parser.parse_args()
  asyncio.run(compare(args.binary.resolve(), args.binding.resolve(), args.output.resolve()))


if __name__ == '__main__':
  main()

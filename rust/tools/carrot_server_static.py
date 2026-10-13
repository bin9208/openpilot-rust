import argparse
import asyncio
from dataclasses import dataclass, field
from email.utils import formatdate
import gzip
import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType

from aiohttp import ClientSession, web
from yarl import URL
from original_params_binding import load


@dataclass(frozen=True)
class Scenario:
  name: str
  path: str = '/plain.txt'
  method: str = 'GET'
  headers: dict[str, str] = field(default_factory=dict)
  etag_header: str = ''
  etag_value: str = ''


BOOTSTRAP = {
  'webSettings': {'language': 'ko', 'nested': '</script>'},
  'webSettingsSpec': {'language': {'default': 'ko'}},
  'webCapabilities': {'camera': True},
  'webCapabilitiesSpec': {'camera': {}},
  'deviceLanguage': 'main_ko',
  'soundLanguage': 'auto',
  'deviceLanguages': [{'code': 'main_ko', 'name': '한국어'}],
  'intro': {'shouldShow': False, 'reason': 'already_completed'},
}


def cases() -> list[Scenario]:
  old = 'Mon, 01 Jan 2001 00:00:00 GMT'
  future = 'Mon, 01 Jan 2040 00:00:00 GMT'
  result = [Scenario(f'range-{name}', headers={'Range': value}) for name, value in (
    ('middle', 'bytes=2-5'), ('suffix', 'bytes=-4'), ('open', 'bytes=4-'),
    ('oversized', 'bytes=0-1000'), ('oversized-suffix', 'bytes=-1000'),
    ('zero-suffix', 'bytes=-0'), ('unsatisfiable', 'bytes=99-100'),
    ('reversed', 'bytes=8-2'), ('multiple', 'bytes=0-1,3-4'),
    ('malformed', 'items=0-1'), ('empty', 'bytes=-'), ('whitespace', 'bytes=0-1 '),
    ('huge', 'bytes=999999999999999999999999-'),
  )]
  result.extend([
    Scenario('range-empty-file', '/empty.txt', headers={'Range': 'bytes=-1'}),
    Scenario('range-head', method='HEAD', headers={'Range': 'bytes=2-5'}),
    Scenario('if-match-success', etag_header='If-Match', etag_value='current'),
    Scenario('if-match-weak', etag_header='If-Match', etag_value='weak'),
    Scenario('if-match-list', etag_header='If-Match', etag_value='list'),
    Scenario('if-match-garbage', etag_header='If-Match', etag_value='garbage'),
    Scenario('if-match-star', headers={'If-Match': '*'}),
    Scenario('unmodified-old', headers={'If-Unmodified-Since': old}),
    Scenario('match-overrides-date', headers={'If-Unmodified-Since': old}, etag_header='If-Match', etag_value='current'),
    Scenario('none-match-weak', etag_header='If-None-Match', etag_value='weak'),
    Scenario('none-match-head', method='HEAD', etag_header='If-None-Match', etag_value='current'),
    Scenario('none-match-star', headers={'If-None-Match': '*'}),
    Scenario('modified-future', headers={'If-Modified-Since': future}),
    Scenario('modified-old', headers={'If-Modified-Since': old}),
    Scenario('none-overrides-date', headers={'If-None-Match': '"different"', 'If-Modified-Since': future}),
    Scenario('range-stale-date', headers={'Range': 'bytes=1-3', 'If-Range': old}),
    Scenario('range-current-date', headers={'Range': 'bytes=1-3', 'If-Range': future}),
    Scenario('range-etag-ignored', headers={'Range': 'bytes=1-3'}, etag_header='If-Range', etag_value='current'),
    Scenario('gzip-range', '/js/app.js', headers={'Accept-Encoding': 'gzip', 'Range': 'bytes=1-6'}),
    Scenario('brotli-priority', '/js/app.js', headers={'Accept-Encoding': 'GZIP, BR;q=0'}),
    Scenario('gzip-conditional', '/js/app.js', headers={'Accept-Encoding': 'gzip'}, etag_header='If-None-Match', etag_value='current'),
    Scenario('brotli-conditional', '/js/app.js', headers={'Accept-Encoding': 'br'}, etag_header='If-None-Match', etag_value='current'),
    Scenario('directory-list', '/folder/'),
    Scenario('directory-no-slash', '/folder'),
    Scenario('shared-forbidden', '/shared-assets/'),
    Scenario('training-forbidden', '/training'),
    Scenario('sound-forbidden', '/sound-assets/'),
    Scenario('shared-range', '/shared-assets/sound.bin', headers={'Range': 'bytes=1-3'}),
    Scenario('training-file', '/training/intro.txt'),
    Scenario('sound-file', '/sound-assets/sound.bin'),
    Scenario('special-file', '/fifo'),
    Scenario('symlink-outside', '/escape.txt'),
    Scenario('symlink-cycle', '/cycle.txt'),
    Scenario('sidecar-symlink', '/plain.txt', headers={'Accept-Encoding': 'br'}),
    Scenario('vendor-error-cache', '/js/vendor/missing.js?v=1'),
    Scenario('static-method', '/js/app.js', method='POST'),
    Scenario('index-ready', '/'), Scenario('index-head', '/', method='HEAD'),
    Scenario('index-repaired-hash', '/'), Scenario('index-asset-change', '/'),
    Scenario('sidecar-stale', '/js/app.js', headers={'Accept-Encoding': 'br'}),
    Scenario('index-missing-manifest', '/'), Scenario('index-malformed-manifest', '/'),
    Scenario('index-outside-manifest', '/'), Scenario('index-duplicate-asset', '/'),
    Scenario('index-missing-placeholder', '/'), Scenario('index-invalid-utf8', '/'),
    Scenario('index-recovered', '/'),
  ])
  return result


def fixture(root: Path) -> tuple[dict[str, str], str, dict]:
  for name in ('web/js', 'web/css', 'web/js/vendor', 'web/generated', 'web/folder/nested', 'shared_assets', 'training_assets', 'data'):
    (root / name).mkdir(parents=True, exist_ok=True)
  files = {'web/plain.txt': b'0123456789', 'web/empty.txt': b'',
           'web/js/app.js': b'export const fixture = 1;\r\n', 'web/css/app.css': b'body { color:red; }\n',
           'web/js/vendor/vendor.js': b'vendor', 'shared_assets/sound.bin': b'abcdef',
           'training_assets/intro.txt': b'training', 'web/folder/a & b.txt': b'listing',
           'web/folder/<quote".txt': b'escaped', 'outside.txt': b'outside'}
  for name, payload in files.items():
    (root / name).write_bytes(payload)
    os.utime(root / name, (1700000123, 1700000123))
  (root / 'web/escape.txt').symlink_to(root / 'outside.txt')
  (root / 'web/cycle.txt').symlink_to('cycle.txt')
  (root / 'web/plain.txt.br').symlink_to(root / 'outside.txt')
  os.mkfifo(root / 'web/fifo')
  source = '''<!doctype html><html><head>
<script id="carrotAssetManifest" type="application/json"></script>
<script src="/js/app.js?x=1&amp;v=old&#38;x=2#frag"></script>
<link HREF='/css/app.css?%76=old&z=3'>
<script src=/js/app.js></script><script>const raw = '<a href="/js/app.js">';</script>
<!-- <link href="/css/app.css"> --><a href="https://example.org/app.js">remote</a>
<a href="//example.org/app.js">remote</a><script src="/js/vendor/vendor.js?v=1"></script>
</head><body>한국어</body></html>'''
  manifest = {'schemaVersion': 1, 'assets': [{'id': 'app.runtime', 'kind': 'bundle', 'source': 'src/app.js', 'path': 'js/app.js',
                                           'hash': hashlib.sha256(files['web/js/app.js'].replace(b'\r\n', b'\n')).hexdigest()}]}
  (root / 'web/index.html').write_text(source, encoding='utf-8')
  (root / 'web/generated/asset-manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
  config = {name: str(root / name) for name in ('web', 'shared_assets', 'training_assets', 'data')}
  config.update(repository=str(root), settings=str(root / 'settings.json'))
  return config, source, manifest


def mutate(name: str, root: Path, source: str, manifest: dict) -> None:
  manifest_path = root / 'web/generated/asset-manifest.json'
  match name:
    case 'index-repaired-hash':
      updated = json.loads(json.dumps(manifest))
      updated['assets'][0]['hash'] = '0' * 64
      manifest_path.write_text(json.dumps(updated))
    case 'index-asset-change':
      (root / 'web/js/app.js').write_bytes(b'updated source for the manifest cache\n')
      os.utime(root / 'web/js/app.js', (1700000223, 1700000223))
    case 'sidecar-stale':
      (root / 'web/js/app.js.br').write_bytes(b'corrupt')
      (root / 'web/js/app.js.gz').write_bytes(b'corrupt')
    case 'index-missing-manifest':
      manifest_path.unlink()
    case 'index-malformed-manifest':
      manifest_path.write_text('{bad')
    case 'index-outside-manifest':
      updated = json.loads(json.dumps(manifest))
      updated['assets'][0]['path'] = '../outside.txt'
      manifest_path.write_text(json.dumps(updated))
    case 'index-duplicate-asset':
      updated = json.loads(json.dumps(manifest))
      updated['assets'].append(updated['assets'][0])
      manifest_path.write_text(json.dumps(updated))
    case 'index-missing-placeholder':
      (root / 'web/index.html').write_text('<head>missing placeholder</head>')
    case 'index-invalid-utf8':
      (root / 'web/index.html').write_bytes(b'\xff')
    case 'index-recovered':
      (root / 'web/index.html').write_text(source, encoding='utf-8')
      manifest_path.write_text(json.dumps(manifest))


def selected(root: Path, scenario: Scenario) -> Path:
  path = scenario.path.split('?', 1)[0]
  if path.startswith('/shared-assets/') or path.startswith('/sound-assets/'):
    file = root / 'shared_assets' / path.split('/', 2)[2]
  elif path.startswith('/training/'):
    file = root / 'training_assets' / path.split('/', 2)[2]
  else:
    file = root / 'web' / path.lstrip('/')
  coding = scenario.headers.get('Accept-Encoding', 'identity').lower()
  for extension, encoding in (('.br', 'br'), ('.gz', 'gzip')):
    sidecar = Path(str(file) + extension)
    if encoding in coding and sidecar.is_file() and not sidecar.is_symlink():
      return sidecar
  return file


async def compare(binary: Path, output: Path, binding: Path) -> None:
  import brotli
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-static-') as temporary:
    root = Path(temporary)
    load(binding, f'ipc://{root}/source-log.sock', output / 'binding-logs')
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.selfdrive.carrot.server.features import static as source_static
    from openpilot.selfdrive.carrot.server.services import static_assets as source_assets
    from openpilot.selfdrive.carrot.server.services.asset_manifest import AssetManifestLoader
    roots = {side: root / side for side in ('original', 'native')}
    fixtures = {side: fixture(directory) for side, directory in roots.items()}
    config, source, manifest = fixtures['original']
    source_static.WEB_DIR = config['web']
    source_static.TRAINING_ASSETS_DIR = config['training_assets']
    source_static.SOUND_ASSETS_DIR = config['shared_assets']
    source_static._ASSET_MANIFEST_LOADER = AssetManifestLoader()
    source_static._build_bootstrap_payload = lambda: BOOTSTRAP
    source_assets.precompress_static_assets(config['web'])
    source_assets._refresh_static_asset(config['web'], '/js/app.js', None)
    absent = {'gzip_matches': gzip.decompress((roots['original'] / 'web/js/app.js.gz').read_bytes()) == (roots['original'] / 'web/js/app.js').read_bytes(),
              'brotli_exists': (roots['original'] / 'web/js/app.js.br').exists()}
    (output / 'original-absent-codec.json').write_text(json.dumps(absent))
    if absent != {'gzip_matches': True, 'brotli_exists': False}:
      raise AssertionError('original absent-codec fixture contract failed')
    source_assets.precompress_static_assets(config['web'])
    app = web.Application(middlewares=[source_assets.create_static_cache_middleware(config['web'])])
    async def header_probe(request: web.Request) -> web.Response:
      return web.json_response({'codepoints': [ord(char) for char in request.headers.get('If-Match', '')]})
    app.router.add_get('/__header_fixture', header_probe)
    source_static.register(app)
    app.router.add_static('/shared-assets/', config['shared_assets'], show_index=False)
    app.router.add_static('/', config['web'], show_index=True)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    source_port = site._server.sockets[0].getsockname()[1]
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
    if child.stdin is None or child.stdout is None or child.stderr is None:
      raise RuntimeError('fixture pipes are required')
    native_config = fixtures['native'][0] | {'bootstrap': BOOTSTRAP, 'precompress': True, 'transport_fixture': True}
    child.stdin.write(json.dumps(native_config) + '\n')
    child.stdin.flush()
    try:
      line = await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10)
      native_port = json.loads(line)['port']
      compression = {}
      for side, directory in roots.items():
        compression[side] = {}
        for asset in ('js/app.js', 'css/app.css', 'js/vendor/vendor.js'):
          file = directory / 'web' / asset
          for ext, decode in (('.gz', gzip.decompress), ('.br', brotli.decompress)):
            sidecar = Path(str(file) + ext)
            encoded = sidecar.read_bytes()
            if decode(encoded) != file.read_bytes():
              raise AssertionError(f'{side} precompression failed for {asset}{ext}')
            os.utime(sidecar, (1700000123, 1700000123))
            compression[side][asset + ext] = {'length': len(encoded), 'decoded_sha256': hashlib.sha256(decode(encoded)).hexdigest()}
      (output / 'precompression.json').write_text(json.dumps(compression, indent=2))
      paired_sidecars = {}
      for asset in ('js/app.js', 'css/app.css', 'js/vendor/vendor.js'):
        original_gzip = (roots['original'] / 'web' / (asset + '.gz')).read_bytes()
        native_gzip = roots['native'] / 'web' / (asset + '.gz')
        native_gzip.write_bytes(original_gzip)
        os.utime(native_gzip, (1700000123, 1700000123))
        paired_sidecars[asset + '.gz'] = hashlib.sha256(original_gzip).hexdigest()
      (output / 'paired-sidecar-inputs.json').write_text(json.dumps(paired_sidecars, indent=2))
      rows = {'original': [], 'native': []}
      async with ClientSession(auto_decompress=False) as session:
        for index, scenario in enumerate(cases()):
          for side, port in (('original', source_port), ('native', native_port)):
            directory = roots[side]
            mutate(scenario.name, directory, source, manifest)
            headers = {'Accept-Encoding': 'identity'} | scenario.headers
            if scenario.etag_header:
              meta = selected(directory, scenario).stat()
              etag = f'"{meta.st_mtime_ns:x}-{meta.st_size:x}"'
              match scenario.etag_value:
                case 'current': value = etag
                case 'weak': value = 'W/' + etag
                case 'list': value = '"different", ' + etag
                case 'garbage': value = 'bad,' + etag
                case _: raise RuntimeError('unknown etag fixture')
              headers[scenario.etag_header] = value
            async with session.request(scenario.method, URL(f'http://127.0.0.1:{port}{scenario.path}', encoded=True), headers=headers) as response:
              body = await response.read()
              capture = {'scenario': scenario.name, 'status': response.status,
                         'headers': dict(response.headers), 'body_hex': body.hex(), 'request_headers': headers}
              (output / f'{index:02}-{side}.json').write_text(json.dumps(capture, ensure_ascii=False, indent=2))
              observed = {name: response.headers.get(name) for name in ('Content-Type', 'Content-Length', 'Content-Encoding', 'Cache-Control', 'Pragma', 'Expires', 'X-Carrot-Asset-Status', 'Retry-After', 'Allow', 'ETag', 'Last-Modified', 'Accept-Ranges', 'Content-Range', 'Vary', 'Transfer-Encoding')}
              if observed['ETag'] is not None:
                meta = selected(directory, scenario).stat()
                expected_etag = f'"{meta.st_mtime_ns:x}-{meta.st_size:x}"'
                if observed['ETag'] != expected_etag or observed['Last-Modified'] != formatdate(math.ceil(meta.st_mtime), usegmt=True):
                  raise AssertionError(f'{side} incorrect selected-representation metadata for {scenario.name}')
                observed['ETag'] = 'selected-representation-metadata'
                observed['Last-Modified'] = 'selected-representation-metadata'
              decoded = body
              if response.status == 200 and body and scenario.method != 'HEAD':
                match observed['Content-Encoding']:
                  case 'br': decoded = brotli.decompress(body)
                  case 'gzip': decoded = gzip.decompress(body)
                  case None: pass
                  case _: raise AssertionError('unexpected static encoding')
              rows[side].append({'scenario': scenario.name, 'status': response.status, 'headers': observed, 'body_hex': decoded.hex()})
      for side, values in rows.items():
        (output / f'{side}.json').write_text(json.dumps(values, indent=2))
      differences = [original['scenario'] for original, native in zip(rows['original'], rows['native'], strict=True) if original != native]
      (output / 'differences.json').write_text(json.dumps(differences))
      header_differences = await header_checks(source_port, native_port, output)
      await transport_checks(binary, native_config, source_port, native_port, output)
      await startup_checks(binary, native_config, roots['original'], output)
      child.stdin.write('stop\n')
      child.stdin.flush()
      code = await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
      if code != 0:
        raise AssertionError(f'native fixture stop exit {code}')
      result = {'passed': not differences and not header_differences, 'differences': differences, 'header_differences': header_differences, 'matched_observations': len(rows['original']) - len(differences), 'http_observations': len(rows['original']), 'precompressed_files': len(compression['native']), 'graceful_stop': code,
                'bootstrap': 'supplied original payload; web settings and intro composition are outside this isolated static fixture',
                'comparison': 'HTTP status, headers, body bytes; representation metadata verified against each side file stat; compressed full bodies decoded'}
      (output / 'result.json').write_text(json.dumps(result, indent=2))
      print(json.dumps(result))
      if differences or header_differences:
        raise AssertionError(f'original static response differences: {differences}; header differences: {header_differences}')
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


def duplicate_range(port: int) -> dict:
  connection = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
  try:
    connection.putrequest('GET', '/plain.txt')
    connection.putheader('Range', 'bytes=0-1 \t')
    connection.putheader('Range', 'bytes=3-4')
    connection.endheaders()
    response = connection.getresponse()
    return {'status': response.status, 'body_hex': response.read().hex()}
  finally:
    connection.close()


def header_wire(port: int, path: str, headers: dict[str, bytes | str]) -> dict:
  connection = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
  try:
    connection.request('GET', path, headers=headers)
    response = connection.getresponse()
    return {'status': response.status, 'headers': {name: response.getheader(name) for name in ('Content-Type', 'Content-Length', 'ETag')}, 'body_hex': response.read().hex()}
  finally:
    connection.close()


async def header_checks(source_port: int, native_port: int, output: Path) -> list[str]:
  invalid = b'"\xff"'
  probe = await asyncio.to_thread(header_wire, source_port, '/__header_fixture', {'If-Match': invalid})
  decoding = json.loads(bytes.fromhex(probe['body_hex']))
  rows = []
  for name, headers in (('obs-text-if-match', {'If-Match': invalid}),
                        ('obs-text-if-none-match-date', {'If-None-Match': invalid, 'If-Modified-Since': 'Mon, 01 Jan 2040 00:00:00 GMT'})):
    original = await asyncio.to_thread(header_wire, source_port, '/plain.txt', headers)
    native = await asyncio.to_thread(header_wire, native_port, '/plain.txt', headers)
    rows.append({'scenario': name, 'original': original, 'native': native})
  capture = {'original_decoding': decoding, 'observations': rows}
  (output / 'header-boundaries.json').write_text(json.dumps(capture, indent=2))
  return [row['scenario'] for row in rows if row['original'] != row['native']]


def transport(port: int, enabled: bool) -> dict:
  values = {'Range': 'bytes=0-1 \t', 'If-Range': 'not-a-date \t', 'If-Match': '"first" \t',
            'If-None-Match': '"second" \t', 'If-Modified-Since': 'Mon, 01 Jan 2040 00:00:00 GMT \t',
            'If-Unmodified-Since': 'Mon, 01 Jan 2001 00:00:00 GMT \t', 'X-Other': 'ordinary \t'}
  connection = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
  rows = []
  try:
    connection.request('POST', '/__transport_fixture', b'length-delimited body', values)
    response = connection.getresponse()
    first = json.loads(response.read())
    if connection.sock is None:
      raise AssertionError('first request closed the keepalive socket')
    descriptor = connection.sock.fileno()
    if first['extension'] != enabled or first['body_hex'] != b'length-delimited body'.hex():
      raise AssertionError('conditional extension/body mismatch')
    for name, value in values.items():
      if first['normal'][name.lower()] != value.rstrip(' \t'):
        raise AssertionError('ordinary HeaderMap was changed')
      expected = value if enabled and name != 'X-Other' else None
      if first['raw'][name.lower()] != expected:
        raise AssertionError('raw conditional header capture mismatch')
    rows.append(first)
    connection.request('POST', '/__transport_fixture', iter((b'chunk-one', b'chunk-two')), {'If-Match': '"next" \t'}, encode_chunked=True)
    response = connection.getresponse()
    second = json.loads(response.read())
    if second['body_hex'] != b'chunk-onechunk-two'.hex() or second['normal']['if-match'] != '"next"':
      raise AssertionError('chunked body or following header was changed')
    if connection.sock is None or connection.sock.fileno() != descriptor:
      raise AssertionError('request body consumption broke keepalive')
    rows.append(second)
    connection.request('GET', '/plain.txt', headers={'Range': 'bytes=4-6'})
    response = connection.getresponse()
    third = {'status': response.status, 'body_hex': response.read().hex()}
    if third != {'status': 206, 'body_hex': b'456'.hex()}:
      raise AssertionError('static request after bodies failed')
    if connection.sock is None or connection.sock.fileno() != descriptor:
      raise AssertionError('static request changed the keepalive socket')
    rows.append(third)
    return {'enabled': enabled, 'same_socket': True, 'requests': rows, 'duplicate_range': duplicate_range(port)}
  finally:
    connection.close()


async def transport_checks(binary: Path, config: dict, source_port: int, native_port: int, output: Path) -> None:
  original_duplicate = await asyncio.to_thread(duplicate_range, source_port)
  enabled = await asyncio.to_thread(transport, native_port, True)
  off_config = config | {'raw_headers': False, 'precompress': False}
  child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  if child.stdin is None or child.stdout is None or child.stderr is None:
    raise RuntimeError('transport fixture pipes required')
  try:
    child.stdin.write(json.dumps(off_config) + '\n')
    child.stdin.flush()
    line = await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10)
    off_port = json.loads(line)['port']
    default = await asyncio.to_thread(transport, off_port, False)
    capture = {'original_duplicate': original_duplicate, 'opt_in': enabled, 'default': default}
    (output / 'transport.json').write_text(json.dumps(capture, indent=2))
    if original_duplicate != enabled['duplicate_range'] or original_duplicate['status'] != 416 or default['duplicate_range']['status'] != 206:
      raise AssertionError('duplicate conditional first-value/default behavior mismatch')
    child.stdin.write('stop\n')
    child.stdin.flush()
    code = await asyncio.wait_for(asyncio.to_thread(child.wait), 10)
    if code != 0:
      raise AssertionError('default transport fixture failed graceful stop')
    (output / 'transport-result.json').write_text(json.dumps({'passed': True, 'default_off': True, 'six_raw_headers': True, 'other_header_unchanged': True,
                                                              'duplicate_first_value': True, 'content_length_body': True, 'chunked_body': True, 'same_keepalive_socket': True}))
  finally:
    if child.poll() is None:
      child.terminate()
      try:
        await asyncio.wait_for(asyncio.to_thread(child.wait), 5)
      except TimeoutError:
        child.kill()
        await asyncio.to_thread(child.wait)
    (output / 'default-transport.stderr').write_text(child.stderr.read())


async def startup_checks(binary: Path, config: dict, original_root: Path, output: Path) -> None:
  rows = []
  for name, invalid in (('missing-shared-root', original_root / 'missing'), ('shared-root-is-file', original_root / 'outside.txt')):
    app = web.Application()
    try:
      app.router.add_static('/shared-assets/', str(invalid), show_index=False)
    except ValueError as error:
      original = str(error)
    else:
      raise AssertionError('original static registration accepted an invalid shared root')
    result = await asyncio.to_thread(subprocess.run, [str(binary)], input=json.dumps(config | {'shared_assets': str(invalid)}) + '\n', text=True, capture_output=True, timeout=10)
    capture = {'scenario': name, 'original_error': original, 'native_exit': result.returncode, 'native_stdout': result.stdout, 'native_stderr': result.stderr}
    rows.append(capture)
    if result.returncode == 0 or result.stdout or original not in result.stderr:
      raise AssertionError('native static startup error differs from original registration')
  (output / 'startup.json').write_text(json.dumps(rows, indent=2))


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  args = parser.parse_args()
  asyncio.run(compare(args.binary, args.output, args.binding))


if __name__ == '__main__':
  main()

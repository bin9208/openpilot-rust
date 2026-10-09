# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
import argparse
import asyncio
from dataclasses import dataclass
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType, SimpleNamespace

from aiohttp import ClientSession, web
from original_params_binding import load


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  body: bytes
  content_type: str | None = 'multipart/form-data; boundary=owned'
  chunked: bool = False


def part(data: bytes, disposition: bytes = b'form-data; name="file"', extra: bytes = b'') -> bytes:
  return b'--owned\r\nContent-Disposition: ' + disposition + b'\r\n' + extra + b'\r\n' + data + b'\r\n'


def cases() -> list[Case]:
  data = b'{"IsMetric": false, "FutureSetting": 12, "Ghost": 4, "LiveParameters": {}}'
  end = b'--owned--\r\n'
  return [
    Case('normal-file', part(data, b'form-data; name="file"; filename="backup.json"') + end),
    Case('filename-absent', part(data) + end),
    Case('filename-irrelevant', part(data, b'form-data; name="file"; filename="bad;name.bin"') + end),
    Case('first-wrong-field', part(b'ignored', b'form-data; name="other"') + part(data) + end),
    Case('first-wrong-truncated', part(b'not complete', b'form-data; name="other"')),
    Case('first-file-only', part(data) + b'--owned\r\nmalformed later field'),
    Case('empty-multipart', end),
    Case('no-disposition', b'--owned\r\nX-Header: ok\r\n\r\n' + data + b'\r\n' + end),
    Case('uppercase-name', part(data, b'FORM-DATA; NAME="file"') + end),
    Case('name-extended', part(data, b"form-data; name*=UTF-8''file") + end),
    Case('name-extended-iso8859-2', part(data, b"form-data; name*=iso-8859-2''file") + end),
    Case('name-extended-unknown-encoding', part(data, b"form-data; name*=owned-unknown-codec''file") + end),
    Case('name-extended-iso8859-2-percent', part(data, b"form-data; name*=iso-8859-2''%66ile") + end),
    Case('name-extended-unknown-encoding-percent', part(data, b"form-data; name*=owned-unknown-codec''%66ile") + end),
    Case('name-continuations', part(data, b'form-data; name*0="fi"; name*1="le"') + end),
    Case('duplicate-name', part(data, b'form-data; name="file"; name="other"') + end),
    Case('duplicate-disposition', part(data, b'form-data; name="file"', b'Content-Disposition: form-data; name="other"\r\n') + end),
    Case('over-32-part-headers', part(data, extra=b''.join(f'X-Owned-{index}: ok\r\n'.encode() for index in range(40))) + end),
    Case('part-header-value-8190', part(data, extra=b'X-Owned: ' + b'a' * 8190 + b'\r\n') + end),
    Case('part-header-value-8191', part(data, extra=b'X-Owned: ' + b'a' * 8191 + b'\r\n') + end),
    Case('part-header-value-8189-ows', part(data, extra=b'X-Owned: ' + b'a' * 8189 + b' \r\n') + end),
    Case('part-header-value-8190-ows', part(data, extra=b'X-Owned: ' + b'a' * 8190 + b'\t\r\n') + end),
    Case('nested-reader', part(b'--child--\r\n', extra=b'Content-Type: multipart/mixed; boundary=child\r\n') + end),
    Case('initial-charset', part(b'utf-8', b'form-data; name="_charset_"') + part(data) + end),
    Case('initial-charset-too-long', part(b'a' * 32, b'form-data; name="_charset_"') + part(data) + end),
    Case('initial-charset-only-closing', part(b'utf-8', b'form-data; name="_charset_"') + end),
    Case('initial-charset-strip-invalid-utf8', part(b' \t\xff ', b'form-data; name="_charset_"') + part(data) + end),
    Case('part-base64-raw', part(b'eyJJc01ldHJpYyI6IGZhbHNlfQ==', extra=b'Content-Transfer-Encoding: base64\r\n') + end),
    Case('part-quoted-printable-raw', part(b'{"CarName":"=41"}', extra=b'Content-Transfer-Encoding: quoted-printable\r\n') + end),
    Case('part-gzip-raw', part(gzip.compress(b'{"IsMetric":false}', mtime=0), extra=b'Content-Encoding: gzip\r\n') + end),
    Case('utf8-replacement', part(b'{"CarName":"bad\xffname"}') + end),
    Case('json-non-object', part(b'[1, 2]') + end),
    Case('json-malformed', part(b'{bad') + end),
    Case('json-empty', part(b'') + end),
    Case('body-truncated', part(data)),
    Case('boundary-not-found', b'not a boundary'),
    Case('missing-content-type', b'' , None),
    Case('wrong-content-type', part(data) + end, 'text/plain'),
    Case('missing-boundary', part(data) + end, 'multipart/form-data'),
    Case('long-boundary', b'', 'multipart/form-data; boundary=' + 'a' * 71),
    Case('mixed-subtype', part(data) + end, 'multipart/mixed; boundary=owned'),
    Case('mixed-content-length', part(data, extra=b'Content-Length: 1\r\n') + end, 'multipart/mixed; boundary=owned'),
    Case('stream-fragmented', part(data) + end, chunked=True),
    Case('over-request-read-limit', part(b' ' * (17 * 1024 * 1024) + b'{"IsMetric": false}') + end),
  ]


def snapshot(store, history: Path) -> dict:
  return {'params': {name: Path(store.get_param_path(name)).read_bytes().hex() if Path(store.get_param_path(name)).is_file() else None for name in ('IsMetric', 'CarName', 'FutureSetting', 'LiveParameters')}, 'history': history.read_text() if history.is_file() else None}


async def compare(binding: Path, binary: Path | None, output: Path, only: list[str], composed: bool) -> None:
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-multipart-') as temporary:
    root = Path(temporary)
    os.environ['CARROT_DATA_DIR'] = str(root / 'data')
    load(binding.resolve(), f'ipc://{root}/logs.sock', output / 'binding-logs')
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.common.params import Params
    from openpilot.selfdrive.carrot.server.features import params as feature
    from openpilot.selfdrive.carrot.server.services import params, settings, param_changes as history
    stores = {side: Params(str(root / side / 'params')) for side in ('original', 'native')}
    for side in stores:
      (root / side / 'state').mkdir()
    catalog_data = {'params': [{'name': 'IsMetric', 'min': 0, 'max': 1, 'default': 1}, {'name': 'CarName', 'default': ''}, {'name': 'FutureSetting', 'min': 0, 'max': 100, 'default': 0}, {'name': 'LiveParameters', 'default': {}}]}
    catalog_path = root / 'catalog.json'
    catalog_path.write_text(json.dumps(catalog_data))
    params.Params = lambda: stores['original']
    settings.settings_cache.update(path=str(catalog_path), data=None, mtime=0)
    history.CARROT_PARAM_CHANGES_PATH = str(root / 'original/state/param_changes.jsonl')
    history.time = SimpleNamespace(time=lambda: 1000)
    app = web.Application(client_max_size=16 * 1024 * 1024)
    app.router.add_post('/api/params_restore', feature.api_params_restore)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    ports = {'original': site._server.sockets[0].getsockname()[1]}
    child = None
    if binary is not None:
      child = subprocess.Popen([str(binary.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
      if child.stdin is None or child.stdout is None or child.stderr is None:
        raise RuntimeError('multipart example pipes are required')
      configuration = {'root': str(root / 'native/params'), 'state': str(root / 'native/state'), 'catalog': catalog_data, 'timestamp': 1000}
      if composed:
        paths = {name: root / name for name in ('repository', 'web', 'shared_assets', 'training_assets', 'legacy_state')}
        for path in paths.values():
          path.mkdir()
        configuration = {name: str(path) for name, path in paths.items()} | {'data': str(root / 'native'), 'params': str(root / 'native/params'), 'settings': str(catalog_path), 'timestamp': 1000}
      child.stdin.write(json.dumps(configuration) + '\n')
      child.stdin.flush()
      ports['native'] = json.loads(await asyncio.wait_for(asyncio.to_thread(child.stdout.readline), 10))['port']
    rows = []
    try:
      async with ClientSession(auto_decompress=False, skip_auto_headers={'Content-Type'}) as session:
        selected = [case for case in cases() if not only or case.name in only]
        if only and set(only) != {case.name for case in selected}:
          raise ValueError('requested multipart case does not exist')
        for index, case in enumerate(selected):
          observations = {}
          for side, port in ports.items():
            store = stores[side]
            for name, data in {'IsMetric': b'1', 'LongitudinalPersonalityMax': b'3', 'CarName': b'old', 'LiveParameters': b'{}'}.items():
              Path(store.get_param_path(name)).write_bytes(data)
            for path in (Path(store.get_param_path('FutureSetting')), root / side / 'state/param_changes.jsonl'):
              path.unlink(missing_ok=True)
            history._known_values.clear()
            async def fragments():
              for start in range(0, len(case.body), 7):
                yield case.body[start:start + 7]
                await asyncio.sleep(0)
            data = fragments() if case.chunked else case.body
            headers = {'Content-Type': case.content_type} if case.content_type is not None else {}
            async with session.post(f'http://127.0.0.1:{port}/api/params_restore', data=data, headers=headers) as response:
              raw = await response.read()
              observations[side] = {'status': response.status, 'headers': [[key.decode('ascii'), value.decode('latin1')] for key, value in response.raw_headers], 'body_hex': raw.hex(), 'json': json.loads(raw), 'files': snapshot(store, root / side / 'state/param_changes.jsonl')}
            (output / f'{index:02}-{case.name}-{side}.json').write_text(json.dumps(observations[side], ensure_ascii=True, indent=2) + '\n')
          normalized = {side: {'status': observation['status'], 'headers': {key.lower(): value for key, value in observation['headers'] if key.lower() in ('content-type', 'content-length', 'content-encoding', 'allow')}, 'body_hex': observation['body_hex'], 'json': observation['json'], 'files': observation['files']} for side, observation in observations.items()}
          row = {'name': case.name, 'input_length': len(case.body), 'input_sha256': hashlib.sha256(case.body).hexdigest(), 'content_type': case.content_type, 'chunked': case.chunked, 'observations': normalized, 'equal': len(normalized) == 1 or normalized['original'] == normalized['native']}
          rows.append(row)
          print(json.dumps({'name': case.name, 'statuses': {side: value['status'] for side, value in observations.items()}, 'equal': row['equal']}), flush=True)
    finally:
      await runner.cleanup()
      if child is not None:
        child.stdin.write('stop\n')
        child.stdin.flush()
        stdout, stderr = await asyncio.wait_for(asyncio.to_thread(child.communicate), 10)
        (output / 'native.stdout').write_text(stdout)
        (output / 'native.stderr').write_text(stderr)
        if child.returncode != 0:
          raise RuntimeError(f'native fixture exit {child.returncode}: {stderr}')
    (output / 'pairs.json').write_text(json.dumps(rows, ensure_ascii=True, indent=2) + '\n')
    result = {'pairs': len(rows), 'differences': [row['name'] for row in rows if not row['equal']], 'source_only': binary is None, 'composed': composed}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
    if result['differences']:
      raise SystemExit(1)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--only', nargs='*', default=[])
  parser.add_argument('--composed', action='store_true')
  args = parser.parse_args()
  asyncio.run(compare(args.binding, args.binary, args.output, args.only, args.composed))


if __name__ == '__main__':
  main()

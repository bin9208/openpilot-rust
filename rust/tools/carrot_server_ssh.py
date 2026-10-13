import argparse
import anyio
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType
from urllib.parse import urlsplit
import zlib

from aiohttp import ClientSession, web
from aiohttp import ClientConnectionResetError
import brotli
from original_params_binding import load


def cases():
  return {
    'empty': ('GET', None), 'head': ('HEAD', None), 'method': ('PUT', {}),
    'malformed': ('POST', b'{bad'), 'non-object': ('POST', []), 'missing': ('POST', {}),
    'surrogate-action': ('POST', {'action': '\ud800'}),
    'remove': ('POST', {'action': 'remove'}), 'refresh-empty': ('POST', {'action': 'refresh'}),
    'invalid-name': ('POST', {'action': 'add', 'username': '-bad'}),
    'surrogate-name': ('POST', {'action': 'add', 'username': '\ud800'}),
    'long-name': ('POST', {'action': 'add', 'username': 'a' * 40}),
    'add': ('POST', {'action': ' ADD ', 'username': ' Owned-User '}), 'saved': ('GET', None),
    'refresh': ('POST', {'action': 'refresh'}),
    'many': ('POST', {'action': 'add', 'username': 'Many'}), 'many-status': ('GET', None),
    'gzip': ('POST', {'action': 'add', 'username': 'Gzip'}),
    'deflate': ('POST', {'action': 'add', 'username': 'Deflate'}),
    'raw-deflate': ('POST', {'action': 'add', 'username': 'Raw-Deflate'}),
    'latin1': ('POST', {'action': 'add', 'username': 'Latin1'}),
    'bad-gzip': ('POST', {'action': 'add', 'username': 'Bad-Gzip'}),
    'bad-utf8': ('POST', {'action': 'add', 'username': 'Bad-UTF8'}),
    'missing-user': ('POST', {'action': 'add', 'username': 'Missing'}),
    'down': ('POST', {'action': 'add', 'username': 'Down'}),
    'no-keys': ('POST', {'action': 'add', 'username': 'No-Keys'}),
    'clear-final': ('POST', {'action': 'remove'}), 'cleared': ('GET', None),
  }


def network_cases():
  return {name.lower(): ('POST', {'action': 'add', 'username': name}) for name in ('Brotli', 'Unknown-Charset', 'Slow')}


async def compare(binary, binding, output, unavailable, memory, network):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-owned-ssh-http-') as temporary:
    root = Path(temporary)
    os.environ['CARROT_DATA_DIR'] = str(root / 'data')
    load(binding, f'ipc://{root}/source-log.sock', output / 'binding-logs')
    package = ModuleType('openpilot.selfdrive.carrot.server.features')
    package.__path__ = [str(Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features')]
    sys.modules[package.__name__] = package
    from openpilot.common.params import Params
    from openpilot.selfdrive.carrot.server.features import ssh_keys as feature
    from openpilot.selfdrive.carrot.server.services import params, ssh_keys
    original = Params(str(root / 'source_params'))
    native = Params(str(root / 'native_params'))
    params.Params = ssh_keys.Params = lambda: original
    params.HAS_PARAMS = ssh_keys.HAS_PARAMS = not memory
    ssh_keys.time.time = lambda: 1700000000
    for name in ('web', 'shared_assets', 'training_assets', 'legacy_state', 'data'):
      (root / name).mkdir()
    (root / 'settings.json').write_text('{"params": []}')
    app = web.Application()
    feature.register(app)
    requests = []
    async def recipient(request):
      name = request.match_info['username']
      requests.append({'path': request.path, 'method': request.method, 'headers': list(request.headers.items())})
      data = b' ssh-ed25519 AAA= owned\n\nssh-rsa !!! bad\n'
      status, headers = 200, {'Content-Type': 'text/plain; charset=utf-8'}
      if name == 'Many':
        data = ('\n'.join(f'{kind} {blob} row' for kind, blob in [('ssh-rsa', 'AAAA===='), ('ssh-rsa', 'AB=='), ('ssh-rsa', 'AA=A'), ('ssh-rsa', 'AA'), ('ssh-ed25519', 'AAA='), ('ecdsa-sha2-nistp256', 'AAAA'), ('sk-ssh-ed25519@openssh.com', 'AAAA'), ('ssh-rsa', 'AAAA'), ('ssh-rsa', 'AAAA'), ('ssh-rsa', 'AAAA')])).encode()
      elif name in ('Gzip', 'Deflate', 'Raw-Deflate'):
        encoding = 'gzip' if name == 'Gzip' else 'deflate'
        data = gzip.compress(data) if name == 'Gzip' else zlib.compress(data)
        if name == 'Raw-Deflate': data = data[2:-4]
        headers['Content-Encoding'] = encoding
      elif name == 'Latin1':
        data = b'ssh-rsa AAA= caf\xe9\x80\n'
        headers['Content-Type'] = 'text/plain; charset=latin-1'
      elif name == 'Bad-Gzip':
        data, headers['Content-Encoding'] = b'not a gzip stream', 'gzip'
      elif name == 'Bad-UTF8': data = b'ssh-rsa AAA= \xff'
      elif name == 'Missing': status = 404
      elif name == 'Down': status = 503
      elif name == 'No-Keys': data = b' \r\n\t'
      elif name == 'Brotli':
        data, headers['Content-Encoding'] = brotli.compress(data), 'br'
      elif name == 'Unknown-Charset':
        headers['Content-Type'] = 'text/plain; charset=not-a-codec'
      elif name == 'Slow':
        response = web.StreamResponse(headers=headers)
        await response.prepare(request)
        try:
          for _ in range(13):
            await response.write(b' ')
            await anyio.sleep(1)
          await response.write(data)
          await response.write_eof()
        except ClientConnectionResetError:
          requests[-1]['recipient_closed_after_timeout'] = True
        return response
      return web.Response(body=data, status=status, headers=headers)
    app.router.add_get('/{username}.keys', recipient)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    port = site._server.sockets[0].getsockname()[1]
    rows = {'original': [], 'native': []}
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    assert child.stdin is not None and child.stdout is not None and child.stderr is not None
    fixture = {name: str(root / name) for name in ('web', 'shared_assets', 'training_assets', 'legacy_state', 'data')}
    fixture.update(repository=str(root), settings=str(root / 'settings.json'), params=str(root / 'native_params'), ssh_endpoint=f'http://127.0.0.1:{port}', ssh_unavailable=unavailable, unavailable=memory)
    child.stdin.write(json.dumps(fixture) + '\n')
    child.stdin.flush()
    try:
      with anyio.fail_after(10): native_port = json.loads(await anyio.to_thread.run_sync(child.stdout.readline))['port']
      async with ClientSession() as client:
        class Recipient:
          def get(self, address, **kwargs):
            parsed = urlsplit(address)
            assert parsed.scheme == 'https' and parsed.netloc == 'github.com', address
            return client.get(f'http://127.0.0.1:{port}{parsed.path}', **kwargs)
        app['http'] = None if unavailable else Recipient()
        for index, (scenario, (method, body)) in enumerate((network_cases() if network else cases()).items()):
          raw = body if isinstance(body, bytes) else json.dumps(body).encode() if method not in ('GET', 'HEAD') else None
          for side, receiver, store in (('original', port, original), ('native', native_port, native)):
            before = len(requests)
            async with client.request(method, f'http://127.0.0.1:{receiver}/api/ssh_keys', data=raw, headers={'Accept-Encoding': 'identity'}) as response:
              body = await response.read()
              row = {'scenario': scenario, 'status': response.status, 'body_hex': body.hex(), 'headers': {key: response.headers.get(key) for key in ('Content-Type', 'Content-Length', 'Allow')}}
              (output / f'{index}-{side}.body').write_bytes(body)
              (output / f'{index}-{side}.headers.json').write_text(json.dumps(list(response.headers.items())))
            row['params'] = {key: Path(store.get_param_path(key)).read_bytes().hex() if Path(store.get_param_path(key)).is_file() else None for key in ('GithubUsername', 'GithubSshKeys', 'GithubSshKeysUpdatedAt')}
            row['outbound'] = [{key: request[key] for key in ('method', 'path')} for request in requests[before:]]
            rows[side].append(row)
        (output / 'outbound.json').write_text(json.dumps(requests, indent=2))
      for side in rows: (output / f'{side}.json').write_text(json.dumps(rows[side], indent=2) + '\n')
      failures = [{'index': index, 'original': left, 'native': right} for index, (left, right) in enumerate(zip(rows['original'], rows['native'])) if left != right]
      (output / 'failures.json').write_text(json.dumps(failures, indent=2))
      child.stdin.write('stop\n')
      child.stdin.flush()
      with anyio.fail_after(10): code = await anyio.to_thread.run_sync(child.wait)
      (output / 'result.json').write_text(json.dumps({'passed': not failures and code == 0, 'cases': len(rows['original']), 'differences': len(failures), 'graceful_exit': code, 'memory': memory, 'client_unavailable': unavailable, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'scope': 'actual source/native SSH HTTP routes, real owned Params and loopback GitHub recipient; header provider identity retained separately'}) + '\n')
      assert not failures and code == 0, (len(failures), code)
    finally:
      await runner.cleanup()
      if child.poll() is None:
        child.terminate()
        with anyio.fail_after(5): await anyio.to_thread.run_sync(child.wait)
      (output / 'native.stderr').write_text(child.stderr.read())


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--unavailable', action='store_true')
  parser.add_argument('--memory', action='store_true')
  parser.add_argument('--network', action='store_true')
  args = parser.parse_args()
  anyio.run(compare, args.binary.resolve(), args.binding, args.output, args.unavailable, args.memory, args.network, backend='asyncio')


if __name__ == '__main__': main()

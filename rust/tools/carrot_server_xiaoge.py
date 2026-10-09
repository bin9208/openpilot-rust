# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
import argparse
from dataclasses import dataclass
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
from pathlib import Path
import socket
import subprocess
import threading
import time
from typing import Literal, TypedDict
import zlib

import aiohttp
from aiohttp import web
import anyio
import brotli
from yarl import URL


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  method: str = 'GET'
  path: str = '/xiaoge/api/status'
  headers: tuple[tuple[str, str], ...] = ()
  body: bytes = b''
  mode: str = ''
  chunked: bool = False
  page: Literal['present', 'missing'] = 'present'


class Outbound(TypedDict):
  method: str
  path: str
  body_hex: str
  content_type: str | None


class Observation(TypedDict):
  status: int
  headers: dict[str, str]
  body_sha256: str
  body_size: int
  outbound: list[Outbound]


def cases() -> list[Case]:
  result = [Case('redirect', path='/xiaoge'), Case('redirect-query', path='/xiaoge?a=1&a=2&q=%ED%95%9C+%20'),
            Case('redirect-head', 'HEAD', '/xiaoge'), Case('page', path='/xiaoge/'),
            Case('page-head', 'HEAD', '/xiaoge/'), Case('page-range', path='/xiaoge/', headers=(('Range', 'bytes=4-15'),)),
            Case('page-range-invalid', path='/xiaoge/', headers=(('Range', 'bytes=9999-'),)),
            Case('page-missing', path='/xiaoge/', page='missing'), Case('page-recovery', path='/xiaoge/'),
            Case('status'), Case('status-query-discarded', path='/xiaoge/api/status?url=http://invalid.test&stream=road'),
            Case('encoded-status', path='/xiaoge/api/%73tatus'), Case('snapshot-default', path='/xiaoge/api/snapshot'),
            Case('snapshot-road', path='/xiaoge/api/snapshot?stream=road&ignored=1'),
            Case('snapshot-encoded', path='/xiaoge/api/snapshot?%73tream=%72oad'),
            Case('snapshot-first-query', path='/xiaoge/api/snapshot?stream=road&stream=wide'),
            Case('snapshot-invalid', path='/xiaoge/api/snapshot?stream=other'),
            Case('snapshot-empty', path='/xiaoge/api/snapshot?stream='),
            Case('config-get', path='/xiaoge/api/config'), Case('config-delete', 'DELETE', '/xiaoge/api/config'),
            Case('config-delete-ignored-body', 'DELETE', '/xiaoge/api/config', body=b'ignored'),
            Case('upstream-close-delete', 'DELETE', '/xiaoge/api/config', mode='close-without-response'),
            Case('upstream-close-post', 'POST', '/xiaoge/api/settings', (('Content-Type', 'application/json'),), b'{}', mode='close-without-response'),
            Case('settings-raw', 'POST', '/xiaoge/api/settings', (('Content-Type', 'application/json'),), b'\xff\x00{"not":"validated"}'),
            Case('unknown', path='/xiaoge/api/unknown')]
  for path, methods in (('/xiaoge', ('POST', 'DELETE')), ('/xiaoge/api/status', ('HEAD', 'POST')),
                        ('/xiaoge/api/snapshot', ('HEAD', 'DELETE')), ('/xiaoge/api/config', ('HEAD', 'PUT')),
                        ('/xiaoge/api/settings', ('GET', 'HEAD', 'DELETE'))):
    result.extend(Case('method-'+method+'-'+path.rsplit('/', 1)[-1], method, path) for method in methods)
  for origin in ('http://{host}', 'https://{host}', '//{host}', '', 'null', 'http://invalid.test', 'http://{host}/path'):
    result.append(Case('origin-'+origin, 'POST', '/xiaoge/api/config', (('Content-Type', 'application/json'), ('Origin', origin)), b'{'))
  for content_type in ('application/json', 'APPLICATION/JSON; charset=latin-1', 'application/json; charset=invalid',
                       'application/problem+json', 'text/plain', ''):
    result.append(Case('content-type-'+content_type, 'POST', '/xiaoge/api/config', (('Content-Type', content_type),), b'not json'))
  for size in (65535, 65536, 65537):
    result.append(Case('request-size-'+str(size), 'POST', '/xiaoge/api/config', (('Content-Type', 'application/json'),), b'x'*size))
  result.append(Case('request-chunked-over-limit', 'POST', '/xiaoge/api/config', (('Content-Type', 'application/json'),), b'x'*65537, chunked=True))
  for encoding, encode in (('gzip', gzip.compress), ('deflate', zlib.compress), ('br', brotli.compress)):
    for size in (65536, 65537):
      result.append(Case('request-'+encoding+'-'+str(size), 'POST', '/xiaoge/api/settings',
                         (('Content-Type', 'application/json'), ('Content-Encoding', encoding)), encode(b'x'*size)))
    result.append(Case('request-invalid-'+encoding, 'POST', '/xiaoge/api/settings',
                       (('Content-Type', 'application/json'), ('Content-Encoding', encoding)), b'invalid'))
  for mode in ('binary', 'missing-type', 'latin1', 'gzip', 'deflate', 'deflate-raw', 'br', 'gzip-trailing', 'gzip-concatenated',
               'invalid-gzip', 'invalid-deflate', 'invalid-br', 'truncated-gzip',
               'truncated-deflate-coalesced', 'truncated-deflate-headers-first', 'truncated-deflate-over-limit-coalesced', 'wrong-length',
               'chunked', 'response-limit', 'response-over-limit', 'gzip-over-limit', 'gzip-over-feed-limit',
               'redirect-301', 'redirect-302', 'redirect-304', 'redirect-307', 'redirect-308',
               'status-201', 'status-204', 'status-400', 'status-500', 'close-without-response',
               'timeout-headers', 'timeout-headers-empty', 'timeout-body-progress', 'near-boundary-success', 'recovery'):
    result.append(Case('upstream-'+mode, mode=mode))
  return result


@dataclass(slots=True)
class PeerState:
  case: Case = Case('initial')
  calls: list[Outbound] | None = None
  side: str = 'source'
  headers_observed: threading.Event | None = None


def recipient_type(state: PeerState) -> type[BaseHTTPRequestHandler]:
  class Recipient(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, format: str, *args: object) -> None:
      pass

    def respond(self) -> None:
      body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
      assert state.calls is not None
      state.calls.append({'method': self.command, 'path': self.path, 'body_hex': body.hex(),
                          'content_type': self.headers.get('Content-Type')})
      mode = state.case.mode
      payload = b'\xff\x00owned diagnosis\n'
      content_type: str | None = 'application/octet-stream'
      encoding = ''
      status = 200
      match mode:
        case 'close-without-response':
          self.close_connection = True
          return
        case 'latin1':
          payload = b'caf\xe9'; content_type = 'text/plain; charset=iso-8859-1'
        case 'missing-type':
          content_type = None
        case 'response-limit':
          payload = b'x'*(4*1024*1024)
        case 'response-over-limit':
          payload = b'x'*(4*1024*1024+1)
        case 'gzip-over-limit':
          payload = b'x'*(4*1024*1024+1); encoding = 'gzip'
        case 'gzip-over-feed-limit':
          payload = b'x'*(33*1024*1024); encoding = 'gzip'
        case 'gzip' | 'gzip-trailing' | 'gzip-concatenated' | 'truncated-gzip' | 'invalid-gzip':
          encoding = 'gzip'
        case 'deflate' | 'deflate-raw' | 'truncated-deflate-headers-first' | 'truncated-deflate-coalesced' | 'invalid-deflate':
          encoding = 'deflate'
        case 'truncated-deflate-over-limit-coalesced':
          payload = b'x'*(4*1024*1024+1); encoding = 'deflate'
        case 'timeout-headers-empty':
          status = 204
        case 'br' | 'invalid-br':
          encoding = 'br'
        case _:
          pass
      match encoding:
        case 'gzip': payload = gzip.compress(payload)
        case 'deflate': payload = zlib.compress(payload)
        case 'br': payload = brotli.compress(payload)
        case '': pass
        case _: raise AssertionError(encoding)
      if mode == 'deflate-raw': payload = payload[2:-4]
      if mode.startswith('invalid-'): payload = b'invalid'
      if mode.startswith('truncated-'): payload = payload[:-5]
      if mode == 'gzip-trailing': payload += b'extra'
      if mode == 'gzip-concatenated': payload += gzip.compress(b'second')
      if mode.startswith('redirect-'): status = int(mode.rsplit('-', 1)[-1])
      if mode.startswith('status-'): status = int(mode.rsplit('-', 1)[-1])
      if mode in ('timeout-headers', 'timeout-headers-empty'): time.sleep(6.2)
      if mode in ('truncated-deflate-coalesced', 'truncated-deflate-over-limit-coalesced'):
        self.wfile.write(b'HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Encoding: deflate\r\nContent-Length: '+str(len(payload)).encode()+b'\r\nConnection: close\r\n\r\n'+payload)
        return
      self.send_response(status)
      if content_type is not None: self.send_header('Content-Type', content_type)
      if encoding: self.send_header('Content-Encoding', encoding)
      if mode.startswith('redirect-'): self.send_header('Location', '/must-not-follow')
      if mode == 'chunked': self.send_header('Transfer-Encoding', 'chunked')
      else: self.send_header('Content-Length', str(len(payload)+10 if mode == 'wrong-length' else len(payload)))
      self.send_header('Connection', 'close')
      try:
        self.end_headers()
        if mode == 'truncated-deflate-headers-first' and state.side == 'source':
          assert state.headers_observed is not None
          assert state.headers_observed.wait(timeout=2), 'source did not observe owned response headers'
        if status in (204, 304): return
        if mode == 'near-boundary-success':
          self.wfile.write(payload[:3]); self.wfile.flush(); time.sleep(5.2); self.wfile.write(payload[3:])
        elif mode == 'timeout-body-progress':
          for part in (payload[:3], payload[3:6], payload[6:9], payload[9:]):
            self.wfile.write(part); self.wfile.flush(); time.sleep(2.4)
        elif mode == 'chunked':
          self.wfile.write(hex(len(payload))[2:].encode()+b'\r\n'+payload+b'\r\n0\r\n\r\n')
        else:
          self.wfile.write(payload)
      except (BrokenPipeError, ConnectionResetError):
        pass

    do_GET = respond
    do_POST = respond
    do_DELETE = respond

  return Recipient


async def chunks(data: bytes):
  for offset in range(0, len(data), 4096):
    yield data[offset:offset+4096]


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--quick', action='store_true')
  parser.add_argument('--refused', action='store_true')
  parser.add_argument('--baseline-slice', action='store_true')
  parser.add_argument('--deflate-boundary', action='store_true')
  parser.add_argument('--composed', action='store_true')
  parser.add_argument('--deadline-boundary', action='store_true')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  spec = importlib.util.spec_from_file_location('xiaoge_source', Path('openpilot/selfdrive/carrot/server/features/xiaoge.py'))
  assert spec is not None and spec.loader is not None
  source = importlib.util.module_from_spec(spec); spec.loader.exec_module(source)
  repository = args.output.resolve()/'repository'
  page = repository/'openpilot/selfdrive/carrot/xiaoge/v_asm_web.html'
  page.parent.mkdir(parents=True, exist_ok=True)
  page_bytes = b'<!doctype html><html><body>owned Xiaoge page\xe2\x98\x83</body></html>\n'
  page.write_bytes(page_bytes)
  if args.composed:
    (repository/'openpilot/selfdrive/carrot/web').mkdir(parents=True, exist_ok=True)
    (repository/'openpilot/selfdrive/assets').mkdir(parents=True, exist_ok=True)
    (repository/'settings.json').write_text('{}\n')
  source.PAGE_PATH = page
  state = PeerState(calls=[])
  recipient = ThreadingHTTPServer(('127.0.0.1', 0), recipient_type(state))
  actor = threading.Thread(target=recipient.serve_forever, daemon=True); actor.start()
  refused_socket = socket.socket(); refused_socket.bind(('127.0.0.1', 0))
  peer_port = refused_socket.getsockname()[1] if args.refused else recipient.server_port
  source.XIAOGE_URL = f'http://127.0.0.1:{peer_port}'
  trace = aiohttp.TraceConfig()
  async def headers_observed(session, context, params) -> None:
    assert state.headers_observed is not None
    state.headers_observed.set()
  trace.on_request_end.append(headers_observed)
  session = aiohttp.ClientSession(trace_configs=[trace])
  app = web.Application(); app['http'] = session; source.register(app)
  runner = web.AppRunner(app); await runner.setup()
  site = web.TCPSite(runner, '127.0.0.1', 0); await site.start()
  source_port = site._server.sockets[0].getsockname()[1]
  native = None; native_port = None
  stderr = (args.output/'native.stderr').open('w')
  if args.binary:
    native = subprocess.Popen([str(args.binary.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
    assert native.stdin is not None and native.stdout is not None
    init = {'repository': str(repository), 'peer': f'127.0.0.1:{peer_port}'}
    if args.composed: init['composed'] = True
    if args.baseline_slice:
      init.update({key: str(repository/key) for key in ('data', 'settings', 'web', 'shared_assets', 'training_assets', 'legacy_state', 'params')})
      for key in ('web', 'shared_assets'): Path(init[key]).mkdir(parents=True, exist_ok=True)
      init['unavailable'] = True
    native.stdin.write(json.dumps(init)+'\n'); native.stdin.flush()
    with anyio.fail_after(15): line = await anyio.to_thread.run_sync(native.stdout.readline)
    native_port = json.loads(line)['port']
  failures = []; observations = []
  selected = [Case('owned-connection-refused')] if args.refused else cases()
  if args.baseline_slice: selected = [Case('missing-redirect', path='/xiaoge'), Case('missing-status')]
  if args.deflate_boundary:
    selected = [Case('upstream-'+mode+'-'+str(index), mode=mode) for index in range(3)
                for mode in ('truncated-deflate-coalesced', 'truncated-deflate-headers-first', 'truncated-deflate-over-limit-coalesced')]
  if args.deadline_boundary:
    selected = [Case('upstream-timeout-headers-empty', mode='timeout-headers-empty')]
  if args.composed and not args.refused:
    selected = [case for case in selected if case.name in ('redirect-query', 'page', 'snapshot-road', 'snapshot-invalid',
                'settings-raw', 'method-HEAD-status', 'origin-http://invalid.test', 'upstream-gzip', 'upstream-redirect-302', 'upstream-recovery')]
  if args.quick: selected = [case for case in selected if not case.mode.startswith('timeout-')]
  try:
    async with aiohttp.ClientSession(auto_decompress=False, timeout=aiohttp.ClientTimeout(total=10)) as client:
      for index, case in enumerate(selected):
        if case.page == 'missing': page.unlink(missing_ok=True)
        else: page.write_bytes(page_bytes)
        state.case = case
        rows: list[Observation] = []
        for side, port in (('source', source_port), ('native', native_port)):
          if port is None: continue
          state.calls = []
          state.side = side
          state.headers_observed = threading.Event()
          if case.mode == 'near-boundary-success':
            await anyio.sleep((0.1-time.monotonic()%1)%1)
          headers = {key: value.replace('{host}', f'127.0.0.1:{port}') for key, value in case.headers}
          started = time.monotonic()
          data = chunks(case.body) if case.chunked else case.body
          async with client.request(case.method, URL(f'http://127.0.0.1:{port}'+case.path, encoded=True),
                                    data=data, headers=headers, allow_redirects=False) as response:
            body = await response.read()
            row: Observation = {'status': response.status,
              'headers': {key.lower(): value for key, value in response.headers.items()
                          if key.lower() in ('content-type', 'content-length', 'content-range', 'location', 'cache-control', 'allow')},
              'body_sha256': hashlib.sha256(body).hexdigest(), 'body_size': len(body), 'outbound': list(state.calls)}
          (args.output/f'{index:03}-{side}.body').write_bytes(body)
          observations.append({'index': index, 'scenario': case.name, 'side': side, 'elapsed_seconds': round(time.monotonic()-started, 3), 'output': row})
          rows.append(row)
        if len(rows) == 2 and rows[0] != rows[1]: failures.append({'index': index, 'scenario': case.name, 'original': rows[0], 'native': rows[1]})
  finally:
    await runner.cleanup(); await session.close()
    recipient.shutdown(); recipient.server_close(); actor.join(); refused_socket.close()
    if native:
      assert native.stdin is not None
      native.stdin.write('stop\n'); native.stdin.flush(); native.stdin.close()
      assert native.wait(timeout=10) == 0
    stderr.close()
  (args.output/'inputs.json').write_text(json.dumps([{'name': case.name, 'method': case.method, 'path': case.path,
    'headers': case.headers, 'body_size': len(case.body), 'body_sha256': hashlib.sha256(case.body).hexdigest(), 'mode': case.mode, 'page': case.page, 'chunked': case.chunked} for case in selected], indent=2)+'\n')
  (args.output/'observations.json').write_text(json.dumps(observations, indent=2)+'\n')
  (args.output/'failures.json').write_text(json.dumps(failures, indent=2)+'\n')
  result = {'passed': bool(args.binary) and not failures, 'source_only': not bool(args.binary), 'cases': len(selected), 'differences': len(failures),
    'surface': 'actual HTTP routes/raw response bytes/selected headers/real owned outbound request bytes',
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary else None}
  (args.output/'result.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result)); assert not failures


if __name__ == '__main__':
  anyio.run(main, backend='asyncio')

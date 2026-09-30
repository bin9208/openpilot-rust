# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "requests==2.34.2", "PyJWT==2.14.0",
#   "cryptography==50.0.1", "urllib3==2.7.0", "charset-normalizer==3.5.1", "brotli==1.2.0", "pyserial==3.5",
# ]
# ///
"""Compare real Params, signed loopback requests, source policy, and collector diagnostics."""

import argparse
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import select
import subprocess
import sys
import threading
import time
import zlib
import brotli
from urllib.parse import parse_qs, urlsplit

import jwt
import zmq
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from openpilot.cereal import log
from logmessaged_native import Peer

ROOT = Path(__file__).resolve().parents[2]


class Server:
  def __init__(self):
    self.rows = []
    self.responses = []
    self.lock = threading.Lock()
    parent = self

    class Handler(BaseHTTPRequestHandler):
      protocol_version = 'HTTP/1.1'

      def log_message(self, *args):
        pass

      def request(self):
        count = int(self.headers.get('content-length', '0'))
        body = self.rfile.read(count)
        with parent.lock:
          parent.rows.append({'line': self.requestline, 'headers': list(self.headers.items()), 'body_hex': body.hex()})
          response = parent.responses.pop(0) if parent.responses else {'status': 403}
        status = response.get('status', 200)
        payload = (
          bytes.fromhex(response['body_hex'])
          if 'body_hex' in response
          else response.get('body', '{"dongle_id":"registered-synthetic"}').encode(response.get('encoding', 'utf-8'))
        )
        if response.get('gzip'):
          payload = gzip.compress(payload)
        elif response.get('deflate'):
          payload = zlib.compress(payload)
          if response['deflate'] == 'raw':
            payload = payload[2:-4]
        elif response.get('brotli'):
          payload = brotli.compress(payload)
        if response.get('disconnect'):
          self.close_connection = True
          return
        if response.get('header_delay'):
          time.sleep(response['header_delay'])
        self.send_response(status)
        content_type = response.get('content_type', 'application/json')
        if content_type is not None:
          self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(payload)))
        self.send_header('Connection', 'close')
        if response.get('gzip'):
          self.send_header('Content-Encoding', 'gzip')
        elif response.get('deflate'):
          self.send_header('Content-Encoding', 'deflate')
        elif response.get('brotli'):
          self.send_header('Content-Encoding', 'br')
        if 'location' in response:
          self.send_header('Location', response['location'])
        extra_headers = response.get('headers', {})
        for key, value in extra_headers.items() if isinstance(extra_headers, dict) else extra_headers:
          self.send_header(key, value)
        self.end_headers()
        try:
          if response.get('body_delay'):
            time.sleep(response['body_delay'])
          if response.get('drip'):
            self.wfile.write(payload[:1])
            self.wfile.flush()
            time.sleep(response['drip'])
            self.wfile.write(payload[1:2])
            self.wfile.flush()
            time.sleep(response['drip'])
            self.wfile.write(payload[2:])
          else:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
          pass
        self.close_connection = True

      do_POST = request
      do_GET = request

    self.http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.thread = threading.Thread(target=self.http.serve_forever, daemon=True)
    self.thread.start()
    self.url = f'http://127.0.0.1:{self.http.server_port}'

  def close(self):
    self.http.shutdown()
    self.http.server_close()
    self.thread.join()


def keys():
  output = {}
  for name, key in [('rsa', rsa.generate_private_key(public_exponent=65537, key_size=2048)), ('ec', ec.generate_private_key(ec.SECP256R1()))]:
    output[name] = {
      'key': key,
      'private': key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL, serialization.NoEncryption()),
      'pkcs8': key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()),
      'public': key.public_key().public_bytes(serialization.Encoding.PEM, serialization.PublicFormat.SubjectPublicKeyInfo),
    }
  return output


def setup(path, case, key_pairs):
  if 'hardware' in case:
    hardware_root = path / 'hardware'
    for name, content in case['hardware'].items():
      destination = hardware_root / name
      destination.parent.mkdir(parents=True, exist_ok=True)
      destination.write_bytes(content)
  comma = path / 'persist/comma'
  comma.mkdir(parents=True)
  params = path / 'params/d'
  params.mkdir(parents=True)
  if 'param' in case:
    (params / 'DongleId').write_bytes(case['param'])
  if 'fallback' in case:
    (comma / 'dongle_id').write_bytes(case['fallback'])
  for name in case.get('keys', ['rsa']):
    pair = key_pairs[name]
    filename = 'id_rsa' if name == 'rsa' else 'id_ecdsa'
    private = case.get('private', pair['pkcs8'] if case.get('pkcs8') else pair['private'])
    public = case.get('public', pair['public'])
    if case.get('crlf'):
      private = private.replace(b'\n', b'\r\n')
      public = public.replace(b'\n', b'\r\n')
    (comma / filename).write_bytes(private)
    if not case.get('no_public'):
      (comma / (filename + '.pub')).write_bytes(public)
      if case.get('public_unreadable'):
        (comma / (filename + '.pub')).chmod(0)
  if case.get('param_directory'):
    (params / 'DongleId').mkdir()
  if case.get('fallback_parent_unreadable'):
    comma.chmod(0)
  return comma.parent, params.parent


def collect(peer, path):
  records = []
  deadline = time.monotonic() + 90
  while True:
    packet = peer.subscribers['logMessage'].receive()
    assert packet is not None, ('collector timeout', path, peer.process.poll())
    with (path / 'logMessage.bin').open('ab') as stream:
      stream.write(packet)
    with log.Event.from_bytes(packet) as event:
      record = json.loads(event.logMessage)
    records.append(record)
    if record['msg'] == 'registration-fixture-end':
      break
    assert time.monotonic() < deadline
  errors = []
  while packet := peer.subscribers['errorLogMessage'].receive(non_blocking=True):
    with (path / 'errorLogMessage.bin').open('ab') as stream:
      stream.write(packet)
    with log.Event.from_bytes(packet) as event:
      errors.append(json.loads(event.errorLogMessage))
  expected_errors = [record for record in records if record['levelnum'] >= 40]
  assert errors == expected_errors
  (path / 'records.json').write_text(json.dumps(records, indent=2))
  return records


def normalized_records(records):
  output = []
  for record in records:
    msg = record['msg']
    exception = bool(record.get('exc_info'))
    output.append([record['levelnum'], msg.split('\n', 1)[0], exception])
  return output


def run_side(side, args, case, path, pairs, server):
  path.mkdir()
  persist, params = setup(path, case, pairs)
  collector_name = 'collector-' + side + '-' + hashlib.sha256(str(path).encode()).hexdigest()[:8]
  collector = Peer(path / 'unused', path / collector_name, original=True)
  collector.start()
  config = dict(
    case.get('config', {}),
    persist=str(persist),
    params=str(params),
    source_root=str(ROOT),
    api_host=server.url,
    endpoint=collector.endpoint,
    binding=str(args.binding),
    log_root=str(collector.root),
  )
  if 'hardware' in case:
    config['hardware_root'] = str(path / 'hardware')
  with server.lock:
    server.rows.clear()
    server.responses = [dict(response) for response in case.get('responses', [{}])]
  (path / 'config.json').write_text(json.dumps(config, indent=2))
  command = [sys.executable, str(ROOT / 'rust/tools/registration_source.py')] if side == 'source' else [*args.runner, str(args.binary)]
  process = None
  started = time.monotonic()
  try:
    with (path / 'stderr.log').open('w') as stderr:
      process = subprocess.Popen(
        command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, env=dict(os.environ, OPENPILOT_PREFIX='d', NO_PROXY='*')
      )
      process.stdin.write(json.dumps(config) + '\n')
      process.stdin.flush()
      assert select.select([process.stdout], [], [], 120)[0], ('registration did not finish within 120s', command)
      result_line = process.stdout.readline()
      assert result_line, (command, process.poll(), (path / 'stderr.log').read_text())
      result = json.loads(result_line)
      records = collect(collector, path)
      process.stdin.write('ack\n')
      process.stdin.flush()
      assert process.wait(timeout=10) == 0
    with server.lock:
      requests = list(server.rows)
    (path / 'requests.json').write_text(json.dumps(requests, indent=2))
    (path / 'result.json').write_text(json.dumps(result, indent=2))
    (path / 'invocation.json').write_text(json.dumps({'argv': command, 'elapsed_seconds': time.monotonic() - started, 'exit': process.returncode}, indent=2))
    return result, records, requests
  finally:
    if process and process.poll() is None:
      process.kill()
      process.wait(timeout=5)
    try:
      if collector.process.poll() is None:
        started_stop = time.monotonic()
        collector.process.send_signal(signal.SIGINT)
        # A blocked C recv may defer Python's signal handler until the next input.
        collector.socket.send_multipart([b'\x0a', b'{"msg":"shutdown-wakeup"}'], flags=zmq.NOBLOCK)
        code = collector.process.wait(timeout=10)
        (collector.output / 'exit.json').write_text(
          json.dumps({'code': code, 'shutdown_seconds': time.monotonic() - started_stop, 'signal': int(signal.SIGINT), 'wake_after_signal': True})
        )
        assert code == -signal.SIGINT
      if 'records' in locals():
        disk = [json.loads(line) for file in sorted(collector.root.glob('swaglog.*')) for line in file.read_text().splitlines()]
        for record in disk:
          record['msg'] = record.pop('msg$s')
          record.pop('id')
        assert disk == [record for record in records if record['levelnum'] >= 20], (path, disk, records)
        (path / 'disk-records.json').write_text(json.dumps(disk, indent=2))
    finally:
      collector.close()
      (persist / 'comma').chmod(0o700)
      for key in (persist / 'comma').iterdir():
        if key.is_file():
          key.chmod(0o600)


def normalized_request(row, pairs, expiration):
  method, target, protocol = row['line'].split()
  parsed = urlsplit(target)
  query = parse_qs(parsed.query, keep_blank_values=True)
  if 'register_token' in query:
    token = query['register_token'][0]
    algorithm = jwt.get_unverified_header(token)['alg']
    pair = pairs['rsa' if algorithm == 'RS256' else 'ec']
    claims = jwt.decode(token, pair['public'], algorithms=[algorithm], options={'verify_exp': False})
    assert claims == {'register': True, 'exp': expiration}, claims
    assert claims['register'] is True and type(claims['exp']) is int, claims
    query['register_token'] = [claims]
    query['algorithm'] = algorithm
    assert query['public_key'] == [pair['public'].decode()], query
  headers = {key.lower(): value for key, value in row['headers']}
  return {
    'method': method,
    'path': parsed.path,
    'query': query,
    'body': row['body_hex'],
    'user_agent': headers.get('user-agent'),
    'accept': headers.get('accept'),
    'accept_encoding': headers.get('accept-encoding'),
    'connection': headers.get('connection'),
    'length': headers.get('content-length'),
    'cookie': headers.get('cookie'),
  }


def cases():
  rows = [
    ('clock_pre_epoch', {'config': {'utc': -1}}),
    ('clock_year_one', {'config': {'utc': -62_135_596_800}}),
    ('clock_last_valid_expiry', {'config': {'utc': 253_402_297_199}}),
    ('clock_expiry_overflow', {'config': {'utc': 253_402_297_200, 'max_sleeps': 2}}),
    ('clock_year_zero', {'config': {'utc': -62_135_596_801, 'max_sleeps': 2}}),
    ('clock_year_ten_thousand', {'config': {'utc': 253_402_300_800, 'max_sleeps': 2}}),
    ('public_open_precedes_private_decode', {'private': b'\xff', 'public_unreadable': True}),
    ('closed_log_missing_key', {'keys': [], 'config': {'closed_log': True}}),
    ('closed_log_invalid_param', {'param': b'\xff', 'config': {'closed_log': True}}),
    ('closed_log_auth_info', {'config': {'closed_log': True, 'spinner': True}}),
    ('closed_log_imei_exception', {'config': {'closed_log': True, 'spinner': True, 'imeis': [{}]}}),
    ('param_write_failure', {'param_directory': True, 'config': {'spinner': True}}),
    ('fallback_stat_permission_error', {'fallback_parent_unreadable': True}),
    ('no_keys_overrides_saved', {'keys': [], 'param': b'old'}),
    ('missing_public', {'no_public': True}),
    ('empty_public', {'public': b''}),
    ('existing_reads_but_does_not_sign', {'param': b'old', 'private': b'not a key'}),
    ('existing_invalid_private_utf8', {'param': b'old', 'private': b'\xff'}),
    ('existing_invalid_public_utf8', {'param': b'old', 'public': b'\xff'}),
    ('persist_fallback', {'fallback': b' \x1csynthetic-fallback\r\n'}),
    ('persist_empty', {'fallback': b' \x1f\r\n', 'config': {'spinner': True}}),
    ('persist_invalid_utf8', {'fallback': b'\xff'}),
    ('invalid_param_falls_back', {'param': b'\xff', 'fallback': b'fallback'}),
    ('empty_param_falls_back', {'param': b'', 'fallback': b'fallback'}),
    ('param_precedes_fallback', {'param': b'present', 'fallback': b'fallback'}),
    ('rsa_signed', {'config': {'spinner': True}}),
    ('rsa_before_ec', {'keys': ['ec', 'rsa']}),
    ('ec_sec1_signed', {'keys': ['ec']}),
    ('ec_pkcs8_signed', {'keys': ['ec'], 'pkcs8': True}),
    ('crlf_keys', {'crlf': True}),
    ('only_second_imei', {'config': {'imeis': [None, 'second +/한'], 'serial': 'serial +/한'}}),
    ('empty_imei_is_available', {'config': {'imeis': ['', None]}}),
    ('both_none_poll_without_sleep', {'config': {'imeis': [None, None, 'first', None], 'step': 31, 'spinner': True}}),
    ('second_imei_error_discards_first', {'config': {'imeis': ['discard', {}, None, 'second'], 'step': 61, 'spinner': True}}),
    ('first_imei_error_skips_second', {'config': {'imeis': [{}, None, 'second'], 'spinner': True}}),
    ('serial_error_has_no_explicit_close', {'config': {'serial': {}, 'spinner': True}}),
    ('spinner_start_error', {'config': {'spinner': True, 'spinner_fail': 'spinner_start'}}),
    ('spinner_update_error', {'config': {'spinner': True, 'spinner_fail': 'spinner_update'}}),
    ('spinner_close_error_prevents_persist', {'config': {'spinner': True, 'spinner_fail': 'spinner_close'}}),
    ('invalid_key_retries_before_http', {'private': b'invalid', 'config': {'max_sleeps': 2, 'spinner': True}}),
    ('retry_backoff_cap', {'responses': [{'body': '{}'}] * 17 + [{}], 'config': {'spinner': True}}),
    ('retry_http_error', {'responses': [{'disconnect': True}, {}]}),
    ('retry_invalid_json', {'responses': [{'body': 'invalid'}, {}]}),
    ('retry_nonobject', {'responses': [{'body': '[]'}, {}]}),
    ('deflate_json', {'responses': [{'deflate': True}]}),
    ('raw_deflate_json', {'responses': [{'deflate': 'raw'}]}),
    ('brotli_json', {'responses': [{'brotli': True}]}),
    ('gzip_json', {'responses': [{'gzip': True}]}),
    ('utf7_valid_shift', {'responses': [{'body': '{"dongle_id":"한글😀"}', 'encoding': 'utf-7', 'content_type': 'application/json; charset=utf-7'}]}),
    ('utf7_malformed_shift', {'responses': [{'body': '{"dongle_id":"+A-"}', 'content_type': 'application/json; charset=utf-7'}]}),
    ('utf7_direct_nonascii', {'responses': [{'body_hex': b'{"dongle_id":"\xff"}'.hex(), 'content_type': 'application/json; charset=utf-7'}]}),
    (
      'utf7_unpaired_surrogate',
      {'config': {'spinner': True}, 'responses': [{'body': '{"dongle_id":"+2AA-"}', 'content_type': 'application/json; charset=utf-7'}]},
    ),
    ('utf7_surrogate_invalid_escape', {'responses': [{'body_hex': b'{"dongle_id":"\\+2AA-"}'.hex(), 'content_type': 'application/json; charset=utf-7'}, {}]}),
    (
      'utf7_surrogate_after_backslash',
      {'config': {'spinner': True}, 'responses': [{'body_hex': b'{"dongle_id":"\\\\+2AA-"}'.hex(), 'content_type': 'application/json; charset=utf-7'}]},
    ),
    ('malformed_cp932', {'responses': [{'body_hex': b'{"dongle_id":"\x82\xa0\x81 "}'.hex(), 'content_type': 'application/json; charset=cp932'}]}),
    ('malformed_shift_jis', {'responses': [{'body_hex': b'{"dongle_id":"\x82\xa0\x81 "}'.hex(), 'content_type': 'application/json; charset=shift_jis'}]}),
    ('explicit_iso8859_1', {'responses': [{'body': '{"dongle_id":"\u0080"}', 'encoding': 'latin1', 'content_type': 'application/json; charset=iso8859-1'}]}),
    ('explicit_latin_1', {'responses': [{'body': '{"dongle_id":"\u0080"}', 'encoding': 'latin1', 'content_type': 'application/json; charset=latin_1'}]}),
    ('explicit_cp819', {'responses': [{'body': '{"dongle_id":"\u0080"}', 'encoding': 'latin1', 'content_type': 'application/json; charset=cp819'}]}),
    ('explicit_utf32_le', {'responses': [{'body': '{"dongle_id":"한글😀"}', 'encoding': 'utf-32-le', 'content_type': 'application/json; charset=utf-32-le'}]}),
    ('explicit_utf32_be', {'responses': [{'body': '{"dongle_id":"한글😀"}', 'encoding': 'utf-32-be', 'content_type': 'application/json; charset=UTF-32-BE'}]}),
    ('explicit_utf32_bom', {'responses': [{'body': '{"dongle_id":"한글😀"}', 'encoding': 'utf-32', 'content_type': 'application/json; charset=utf-32'}]}),
    (
      'explicit_utf32_replace',
      {
        'responses': [
          {
            'body_hex': ('{"dongle_id":"'.encode('utf-32-le') + bytes.fromhex('00d8000000001100') + '"}'.encode('utf-32-le')).hex(),
            'content_type': 'application/json; charset=utf-32-le',
          }
        ]
      },
    ),
    (
      'explicit_unknown_utf8_fallback',
      {'responses': [{'body': '{"dongle_id":"\u0080"}', 'encoding': 'latin1', 'content_type': 'application/json; charset=unknown-encoding'}]},
    ),
    ('no_content_type_ascii', {'responses': [{'content_type': None}]}),
    ('no_content_type_korean', {'responses': [{'body': '{"dongle_id":"한글"}', 'content_type': None}]}),
    ('no_content_type_utf8_bom', {'responses': [{'body': '{"dongle_id":"café"}', 'encoding': 'utf-8-sig', 'content_type': None}]}),
    ('no_content_type_utf16_bom', {'responses': [{'body': '{"dongle_id":"한글"}', 'encoding': 'utf-16', 'content_type': None}]}),
    ('no_content_type_cp1251', {'responses': [{'body': '{"dongle_id":"идентификатор"}', 'encoding': 'cp1251', 'content_type': None}]}),
    ('no_content_type_cp949', {'responses': [{'body': '{"dongle_id":"한글"}', 'encoding': 'cp949', 'content_type': None}]}),
    ('no_content_type_latin1', {'responses': [{'body': '{"dongle_id":"café"}', 'encoding': 'latin1', 'content_type': None}]}),
    ('no_content_type_utf8', {'responses': [{'body': '{"dongle_id":"café"}', 'content_type': None}]}),
    ('json_default_utf8', {'responses': [{'body': '{"dongle_id":"café"}', 'encoding': 'latin1'}]}),
    ('html_default_latin1', {'responses': [{'body': '{"dongle_id":"café"}', 'encoding': 'latin1', 'content_type': 'text/html'}]}),
    ('latin1_json', {'responses': [{'body': '{"dongle_id":"café"}', 'encoding': 'latin1', 'content_type': 'text/plain'}]}),
    ('utf16_json', {'responses': [{'body': '{"dongle_id":"한글"}', 'encoding': 'utf-16', 'content_type': 'application/json; charset=utf-16'}]}),
    ('utf8_bom_sig_json', {'responses': [{'body': '{"dongle_id":"한글"}', 'encoding': 'utf-8-sig', 'content_type': 'application/json; charset=utf-8-sig'}]}),
    ('utf8_json', {'responses': [{'body': '{"dongle_id":"한글"}'}]}),
  ]
  for status in [200, 201, 400, 401, 402, 403, 404, 500]:
    rows.append((f'status_{status}', {'responses': [{'status': status, 'body': 'not json' if status in (402, 403) else '{"dongle_id":"status-id"}'}]}))
  for status in [301, 302, 303, 307, 308]:
    rows.append((f'redirect_{status}', {'responses': [{'status': status, 'location': '/redirected'}, {}]}))
  rows.append(('redirect_cookie', {'responses': [{'status': 302, 'location': '/cookie', 'headers': {'Set-Cookie': 'test=synthetic; Path=/'}}, {}]}))
  for cookie in [
    'test=deleted; Max-Age=0; Path=/',
    'test=old; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/',
    'test=secure; Secure; Path=/',
    'test=path; Path=/elsewhere',
    'test=domain; Domain=example.invalid; Path=/',
  ]:
    rows.append((f'cookie_applicability_{len(rows)}', {'responses': [{'status': 302, 'location': '/cookie', 'headers': {'Set-Cookie': cookie}}, {}]}))
  rows.append(
    (
      'cookie_path_order',
      {
        'responses': [
          {'status': 302, 'location': '/cookie/next', 'headers': [['Set-Cookie', 'root=first; Path=/'], ['Set-Cookie', 'specific=second; Path=/cookie']]},
          {},
        ]
      },
    )
  )
  rows.append(
    (
      'cookie_delete',
      {
        'responses': [
          {'status': 302, 'location': '/cookie', 'headers': {'Set-Cookie': 'test=value; Path=/'}},
          {'status': 302, 'location': '/cookie/next', 'headers': {'Set-Cookie': 'test=deleted; Max-Age=0; Path=/'}},
          {},
        ]
      },
    )
  )
  rows.append(('cookie_reset_on_retry', {'responses': [{'body': 'invalid', 'headers': {'Set-Cookie': 'test=value; Path=/'}}, {}]}))
  rows.append(('redirect_limit', {'responses': [{'status': 302, 'location': '/loop'}] * 31 + [{}]}))
  for value in ['null', 'false', '0', '-0.0', '""', '[]', '{}', '1', 'true', '[1]', '{"a":1}', 'NaN', '"\\ud800"']:
    rows.append((f'json_value_{len(rows)}', {'responses': [{'body': '{"dongle_id":' + value + '}'}], 'config': {'spinner': True}}))
  for value in [None, b'', b'\xff', b'UnregisteredDevice', b'registered']:
    case = {'config': {'mode': 'is_registered'}}
    if value is not None:
      case['param'] = value
    rows.append((f'is_registered_{len(rows)}', case))
  return rows


def hardware_cases():
  rows = []
  values = {
    'string': '"fixture +/한"',
    'null': 'null',
    'empty': '""',
    'false': 'false',
    'integer': '123456789012345678901234567890',
    'float': '-0.0',
    'array': '[null, false, "", ["한", null], {"quote\\\"":true}, NaN]',
    'object': '{"a +/한":7,"second":null}',
    'empty_array': '[]',
    'surrogate_query': '"\\ud800"',
  }
  for name, value in values.items():
    rows.append(('hardware_' + name, {
      'hardware': {'proc/cmdline': b'androidboot.serialno=fixture', 'dev/shm/modem': ('{"imei":' + value + '}').encode()},
      'config': {'spinner': name != 'surrogate_query', 'step': 61, 'max_sleeps': 1},
    }))
  rows.append(('hardware_modem_error', {
    'hardware': {'proc/cmdline': b'androidboot.serialno=fixture', 'dev/shm/modem': b'[]'},
    'config': {'spinner': True, 'step': 61, 'max_sleeps': 1},
  }))
  rows.append(('hardware_serial_error', {'hardware': {'proc/cmdline': b'other=value'}, 'config': {'spinner': True}}))
  return rows


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('binding', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  parser.add_argument('--case')
  parser.add_argument('--timeouts', action='store_true')
  parser.add_argument('--hardware', action='store_true')
  args = parser.parse_args()
  args.binary = args.binary.resolve()
  args.binding = args.binding.resolve()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True)
  pairs = keys()
  server = Server()
  results = []
  try:
    selected_cases = hardware_cases() if args.hardware else cases() + hardware_cases()
    if args.timeouts:
      selected_cases = [
        ('header_timeout_15s', {'responses': [{'header_delay': 16}, {}], 'minimum_seconds': 15}),
        ('body_timeout_15s', {'responses': [{'body_delay': 16}, {}], 'minimum_seconds': 15}),
        ('body_progress_exceeds_15s', {'responses': [{'drip': 8}], 'minimum_seconds': 16}),
      ]
    for name, case in selected_cases:
      if args.case and name != args.case:
        continue
      path = args.output / name
      path.mkdir()
      source = run_side('source', args, case, path / 'source', pairs, server)
      native = run_side('native', args, case, path / 'native', pairs, server)
      for side in [source, native]:
        side[0]['outcome'].pop('detail', None)
        side[0]['outcome'].pop('type', None)
        if 'value_json' in side[0]['outcome']:
          value = json.loads(side[0]['outcome'].pop('value_json'))
          side[0]['outcome']['value'] = value
        if 'value' in side[0]['outcome']:
          side[0]['outcome']['value_type'] = type(side[0]['outcome']['value']).__name__
      for side_name in ['source', 'native']:
        invocation = json.loads((path / side_name / 'invocation.json').read_text())
        assert invocation['elapsed_seconds'] >= case.get('minimum_seconds', 0)
      assert source[0] == native[0], (name, source[0], native[0])
      assert normalized_records(source[1]) == normalized_records(native[1]), (name, normalized_records(source[1]), normalized_records(native[1]))
      source_requests = [normalized_request(row, pairs, case.get('config', {}).get('utc', 1_700_000_000) + 3600) for row in source[2]]
      native_requests = [normalized_request(row, pairs, case.get('config', {}).get('utc', 1_700_000_000) + 3600) for row in native[2]]
      assert source_requests == native_requests, (name, source_requests, native_requests)
      result = {'case': name, 'result': 'PASS', 'requests': len(source_requests), 'records': len(source[1]), 'outcome': source[0]['outcome']}
      results.append(result)
      (path / 'comparison.json').write_text(json.dumps(result, indent=2))
      print(name, 'PASS', flush=True)
    assert results
    (args.output / 'result.json').write_text(
      json.dumps({'result': 'PASS', 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'cases': results}, indent=2)
    )
  finally:
    server.close()


if __name__ == '__main__':
  main()

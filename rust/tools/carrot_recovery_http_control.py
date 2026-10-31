from __future__ import annotations

import argparse
import base64
import hashlib
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import time

import anyio
from aiohttp import ClientSession
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_upload import save


async def exchange(port: int, wire: bytes, expect: bool = False) -> dict:
  stream = await anyio.connect_tcp('127.0.0.1', port)
  reader = BufferedByteReceiveStream(stream)
  began = time.monotonic()
  try:
    await stream.send(wire)
    interim = None
    if expect:
      with anyio.fail_after(2):
        interim = await reader.receive_until(b'\r\n\r\n', 65536)
      assert interim == b'HTTP/1.1 100 Continue'
      await stream.send(b'{}')
    with anyio.fail_after(2):
      row = await read_response(reader, 'POST' if expect else 'GET')
    row['headers'] = list(row['headers'].items())
    row['seconds'] = time.monotonic() - began
    row['interim'] = interim.decode() if interim else None
    return row
  finally:
    await stream.aclose()


def normalized(row: dict) -> dict:
  headers = {key.lower(): value for key, value in row['headers']}
  return {
    'status': row['status'],
    'body': row['body_base64'],
    'headers': {key: value for key, value in headers.items() if key not in {'date', 'server'}},
    'interim': row.get('interim'),
  }


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  root.mkdir(parents=True)
  native_binary = args.binary.resolve()
  native_repository = Path.cwd()
  if args.standalone:
    package = root / 'minimal-host-package'
    assets = Path('openpilot/selfdrive/carrot/web').resolve()
    helper = native_binary.with_name('openpilot-process-child')
    growth = native_binary.stat().st_size + helper.stat().st_size + sum(path.stat().st_size for path in assets.rglob('*') if path.is_file())
    free = shutil.disk_usage(root).free
    assert free >= growth + 25 * 1024**2, (free, growth)
    (package / 'bin').mkdir(parents=True)
    for binary in [native_binary, helper]:
      shutil.copy2(binary, package / 'bin' / binary.name)
    native_repository = package / 'repository'
    shutil.copytree(assets, native_repository / 'openpilot/selfdrive/carrot/web', ignore=shutil.ignore_patterns('*.py', '__pycache__'))
    native_binary = package / 'bin' / native_binary.name
    dynamic = subprocess.run(['readelf', '-d', str(native_binary)], check=True, capture_output=True, text=True).stdout
    needed = re.findall(r'\(NEEDED\).*\[(.*?)\]', dynamic)
    assert set(needed) <= {'libstdc++.so.6', 'libgcc_s.so.1', 'libm.so.6', 'libc.so.6', 'ld-linux-x86-64.so.2'}, needed
    assert not (package / 'bin/openpilot-carrot-server').exists()
    assert not list(native_repository.rglob('*.py'))
    save(root / 'standalone.json', {'needed': needed, 'free_before_copy': free, 'estimated_growth': growth, 'main_web_absent': True, 'python_runtime_files_absent': True})
  peers = []
  captures = []
  try:
    for provider in ['source', 'native']:
      output = root / provider
      output.mkdir()
      with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
      command = (
        [sys.executable, '-P', str(Path(__file__).with_name('carrot_recovery_source.py')), '--port', str(port), '--output', str(output)]
        if provider == 'source'
        else [str(native_binary), '--host', '127.0.0.1', '--port', str(port)]
      )
      log = (output / 'process.log').open('wb')
      env = {**os.environ, 'OPENPILOT_ROOT': str(Path.cwd() if provider == 'source' else native_repository)}
      process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, env=env)
      peers.append((provider, process, port, log))
      save(
        output / 'invocation.json',
        {'command': command, 'pid': process.pid, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if provider == 'native' else None},
      )
    async with ClientSession() as client:
      for _, _, port, _ in peers:
        with anyio.fail_after(5):
          while True:
            try:
              async with client.get(f'http://127.0.0.1:{port}/api/support_terminal/status') as response:
                assert response.status == 200
                break
            except OSError as error:
              save(root / 'startup-retry.json', {'type': type(error).__name__, 'message': str(error)})
            await anyio.sleep(0.01)
      if args.standalone:
        native_port = next(port for provider, _, port, _ in peers if provider == 'native')
        env = {**os.environ, 'OPENPILOT_ROOT': str(native_repository)}
        help_result = await anyio.run_process([str(native_binary), '--help'], env=env)
        invalid = await anyio.run_process([str(native_binary), '--port', 'invalid'], env=env, check=False)
        duplicate = await anyio.run_process([str(native_binary), '--host', '127.0.0.1', '--port', str(native_port)], env=env, check=False)
        assert help_result.returncode == 0 and invalid.returncode != 0 and duplicate.returncode != 0
        save(root / 'standalone-cli.json', {'help': help_result.returncode, 'invalid_port': invalid.returncode, 'duplicate_listener': duplicate.returncode})
      for method, path in [
        ('GET', '/'),
        ('HEAD', '/'),
        ('GET', '/recovery.js'),
        ('GET', '/recovery-api.js'),
        ('GET', '/css/tokens.css'),
        ('HEAD', '/css/tokens.css'),
        ('GET', '/missing'),
        ('HEAD', '/missing'),
        ('GET', '/api/support_terminal/status'),
        ('HEAD', '/api/support_terminal/status'),
        ('GET', '/download/tmux.log'),
      ]:
        pair = []
        for _, _, port, _ in peers:
          async with client.request(method, f'http://127.0.0.1:{port}' + path) as response:
            pair.append({'status': response.status, 'headers': list(response.headers.items()), 'body_base64': base64.b64encode(await response.read()).decode()})
        captures.append({'method': method, 'path': path, 'pair': pair})
        save(root / 'http.json', captures)
        assert normalized(pair[0]) == normalized(pair[1]), captures[-1]
      mismatches = []
      for name, wire, expected in [
        ('no-newline-request', b'G' * 65537, 414),
        ('no-newline-header', b'GET / HTTP/1.1\r\nX-Test: ' + b'a' * 65537, 431),
        ('expect-continue', b'POST /missing HTTP/1.1\r\nHost: owned\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n', 404),
        ('empty-content-length', b'POST /missing HTTP/1.1\r\nContent-Length: \r\nConnection: close\r\n\r\n', 404),
        ('invalid-content-length', b'POST /missing HTTP/1.1\r\nContent-Length: invalid\r\nConnection: close\r\n\r\n', 400),
        ('signed-content-length', b'POST /missing HTTP/1.1\r\nContent-Length: +2\r\nConnection: close\r\n\r\n{}', 404),
        ('invalid-utf8', b'POST /missing HTTP/1.1\r\nContent-Length: 1\r\nConnection: close\r\n\r\n\xff', 400),
        ('truncated-utf8', b'POST /missing HTTP/1.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n\xe2\x82', 400),
        ('invalid-json', b'POST /missing HTTP/1.1\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{', 400),
        ('json-body-at-limit', b'POST /missing HTTP/1.1\r\nContent-Length: 65536\r\nConnection: close\r\n\r\n"' + b'a' * 65534 + b'"', 404),
        ('json-body-over-limit', b'POST /missing HTTP/1.1\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n', 400),
      ]:
        pair = [await exchange(port, wire, name == 'expect-continue') for _, _, port, _ in peers]
        captures.append({'scenario': name, 'pair': pair})
        save(root / 'http.json', captures)
        if normalized(pair[0]) != normalized(pair[1]) or any(row['status'] != expected for row in pair):
          mismatches.append(captures[-1])
      assert not mismatches, mismatches
      for path, expected in [
        ('/api/support_terminal/stop', 200),
        ('/api/support_terminal/commands/missing/approve', 400),
        ('/api/support_terminal/commands/missing/reject', 400),
      ]:
        pair = []
        for _, _, port, _ in peers:
          async with client.post(f'http://127.0.0.1:{port}' + path, json={}) as response:
            pair.append({'status': response.status, 'headers': list(response.headers.items()), 'body_base64': base64.b64encode(await response.read()).decode()})
        captures.append({'method': 'POST', 'path': path, 'pair': pair})
        save(root / 'http.json', captures)
        assert normalized(pair[0]) == normalized(pair[1]) and all(row['status'] == expected for row in pair), captures[-1]
  finally:
    with anyio.CancelScope(shield=True):
      exits = []
      for provider, process, _, log in peers:
        if provider == 'source':
          await process.stdin.send(b'\n')
          await process.stdin.aclose()
        else:
          process.send_signal(signal.SIGTERM)
        with anyio.fail_after(5):
          await process.wait()
        exits.append({'provider': provider, 'exit': process.returncode})
        await process.aclose()
        log.close()
      save(root / 'cleanup.json', exits)
  assert all(row['exit'] == 0 for row in exits)
  save(
    root / 'result.json',
    {
      'http_pairs': len(captures),
      'bounded_no_newline_pair': True,
      'expect_continue_pair': True,
      'scope': 'bootstrap/static/parser and inactive support routes',
      'standalone_minimal_host': args.standalone,
    },
  )
  print('PASS')


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--standalone', action='store_true')
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)

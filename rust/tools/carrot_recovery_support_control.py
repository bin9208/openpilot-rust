from __future__ import annotations

import argparse
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import queue
import re
import shlex
import signal
import socket
import subprocess
import sys
import threading
import time
from urllib.parse import urlparse

import anyio
from aiohttp import ClientSession
from carrot_recovery_terminal_control import connect, save


class Webhook:
  def __init__(self) -> None:
    self.messages = queue.Queue()
    messages = self.messages
    class Handler(BaseHTTPRequestHandler):
      def do_POST(self) -> None:
        body = self.rfile.read(int(self.headers['Content-Length']))
        messages.put({'path': self.path, 'headers': list(self.headers.items()), 'body': json.loads(body)})
        self.send_response(204)
        self.end_headers()
      def log_message(self, *args) -> None:
        pass
    self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.worker = threading.Thread(target=self.server.serve_forever)
    self.worker.start()
    self.url = f'http://127.0.0.1:{self.server.server_port}/owned-webhook'

  async def message(self) -> dict:
    with anyio.fail_after(5):
      while True:
        try:
          return self.messages.get_nowait()
        except queue.Empty:
          await anyio.sleep(0.01)

  async def close(self) -> None:
    await anyio.to_thread.run_sync(self.server.shutdown)
    self.server.server_close()
    self.worker.join()


async def exercise(port: int, output: Path, webhook: Webhook) -> dict:
  wire_rows = []
  http_rows = []
  webhook_rows = []
  peers = []
  results = {}
  static = []
  base = f'http://127.0.0.1:{port}'
  async with ClientSession() as client:
    async def request(method: str, url: str, body: object = None) -> tuple[int, object]:
      async with client.request(method, url, json=body, headers={'Host': 'owned'}) as response:
        raw = await response.read()
        http_rows.append({'method': method, 'url': url, 'status': response.status, 'headers': list(response.headers.items()), 'body_base64': base64.b64encode(raw).decode()})
        try:
          payload = json.loads(raw)
        except json.JSONDecodeError:
          payload = raw
        return response.status, payload

    async def start(mode: str) -> tuple[dict, int, str]:
      code, session = await request('POST', base + '/api/support_terminal/start', {'note': 'owned support scenario', 'permission_mode': mode, 'ttl_seconds': 900, 'command_timeout_seconds': 15})
      assert code == 200 and session['active'] and session['state'] == 'sharing'
      assert session['permission_mode'] == mode and session['command_timeout_seconds'] == 15 and session['ttl_seconds'] == 900
      message = await webhook.message()
      webhook_rows.append(message)
      pin = re.search(r'PIN: \*\*__(\d{6})__\*\*', message['body']['content'])[1]
      assert session['id'] in message['body']['content'] and session['url'] in message['body']['content']
      assert message['body']['allowed_mentions'] == {'parse': []}
      for expected in ('Car name: owned-support-car', 'DongleId: owned-support-id', 'Serial: owned-support-serial'):
        assert expected in message['body']['content']
      guest_port = urlparse(session['local_url']).port
      return session, guest_port, pin

    async def guest(guest_port: int, sid: str, pin: str, granted: bool) -> object:
      peer, handshake = await connect(guest_port, wire_rows, path='/ws/support_terminal/' + sid)
      peers.append(peer)
      assert handshake.startswith(b'HTTP/1.1 101 ')
      await peer.send({'type': 'auth', 'pin': pin})
      auth = await peer.event('auth_ok')
      assert auth['control_granted'] is granted
      assert (await peer.event('meta'))['session'] == 'recovery-shell'
      return peer

    async def owner() -> object:
      peer, _ = await connect(port, wire_rows, path='/ws/support_terminal/owner')
      peers.append(peer)
      assert (await peer.event('session_status'))['owner_present']
      return peer

    async def stop(active: list, guest_port: int) -> None:
      code, stopped = await request('POST', base + '/api/support_terminal/stop', {})
      assert code == 200 and stopped == {'ok': True, 'active': False, 'state': 'idle'}
      for peer in active:
        assert (await peer.event('session_closed'))['reason'] == 'user'
        opcode, _ = await peer.frame()
        assert opcode == 8
        await peer.stream.aclose()
        peers.remove(peer)
      with anyio.fail_after(3):
        while True:
          try:
            probe = await anyio.connect_tcp('127.0.0.1', guest_port)
            await probe.aclose()
          except OSError:
            break
          await anyio.sleep(0.01)

    try:
      idle, _ = await connect(port, wire_rows, path='/ws/support_terminal/owner')
      peers.append(idle)
      assert await idle.event('session_status') == {'type': 'session_status', 'ok': True, 'active': False, 'state': 'idle'}
      assert (await idle.frame())[0] == 8
      await idle.stream.aclose()
      peers.remove(idle)
      results['owner_without_session_closes'] = True
      session, guest_port, pin = await start('approve_each')
      code, duplicate = await request('POST', base + '/api/support_terminal/start', {'permission_mode': 'allow_all', 'note': 'duplicate'})
      assert code == 200 and duplicate['id'] == session['id'] and duplicate['permission_mode'] == 'approve_each'
      results['duplicate_start_preserves_session'] = True
      guest_base = f'http://127.0.0.1:{guest_port}'
      for path in ['/support/terminal/' + session['id'], '/support-terminal-assets/xterm.js', '/api/support_terminal/status', '/api/terminal/input', '/download/tmux.log']:
        code, body = await request('GET', guest_base + path)
        expected = 200 if path.startswith('/support') else 404
        assert code == expected
        row = http_rows[-1]
        headers = {key.lower(): value for key, value in row['headers']}
        if path.startswith('/support/terminal/'):
          assert headers['x-frame-options'] == 'DENY' and headers['x-content-type-options'] == 'nosniff'
          assert "connect-src 'self' ws://owned wss://owned" in headers['content-security-policy']
          normalized = body.replace(session['id'].encode(), b'OWNED_SESSION')
          static.append({'path': '/support/terminal/OWNED_SESSION', 'sha256': hashlib.sha256(normalized).hexdigest(), 'headers': {key: value for key, value in headers.items() if key not in {'date', 'server', 'content-length'}}})
        elif code == 200:
          static.append({'path': path, 'sha256': hashlib.sha256(body).hexdigest()})
      results['guest_static_csp_and_api_isolation'] = True

      bad, _ = await connect(guest_port, wire_rows, path='/ws/support_terminal/' + session['id'])
      peers.append(bad)
      wrong = '000000' if pin != '000000' else '999999'
      for remaining in [4, 3, 2, 1, 0]:
        await bad.send({'type': 'auth', 'pin': wrong})
        assert await bad.event('auth_failed') == {'type': 'auth_failed', 'remaining': remaining}
      assert (await bad.frame())[0] == 8
      await bad.stream.aclose()
      peers.remove(bad)
      results['five_bad_pin_attempts_close_peer'] = True
      remote = await guest(guest_port, session['id'], pin, True)
      denied = output / 'owner-absent-denied'
      await remote.send({'type': 'input', 'data': 'touch ' + shlex.quote(str(denied))})
      assert await remote.event('owner_absent') == {'type': 'owner_absent'}
      assert not denied.exists()
      await remote.send({'type': 'raw', 'data': 'touch ' + shlex.quote(str(denied)) + '\r'})
      assert await remote.event('owner_absent') == {'type': 'owner_absent'}
      results['owner_absent_input_and_raw_denied'] = True
      host = await owner()
      assert (await remote.event('owner_presence'))['active'] is True
      await host.send({'type': 'typing', 'active': True, 'text': 'a\nb\tc'})
      assert await remote.event('host_typing') == {'type': 'host_typing', 'active': True, 'text': 'abc'}
      await remote.send({'type': 'typing', 'active': True, 'text': 'x\ny\tz'})
      assert await host.event('guest_typing') == {'type': 'guest_typing', 'active': True, 'text': 'xyz'}
      text = 'A\u202eB\u200dC\u0378D\ufeffE\ue000F\ud800G\u00a0 H'
      for sender, receiver, event in [(host, remote, 'host_typing'), (remote, host, 'guest_typing')]:
        await sender.send({'type': 'typing', 'active': True, 'text': text})
        assert await receiver.event(event) == {'type': event, 'active': True, 'text': 'ABCDEFG H'}
      results['typing_sanitizes_controls'] = True
      await remote.send({'type': 'raw', 'data': 'ignored'})
      assert await remote.event('input_denied') == {'type': 'input_denied', 'reason': 'approval required'}
      results['approve_each_raw_denied'] = True

      approved = output / 'approved.txt'
      line = 'printf "263-approved" > ' + shlex.quote(str(approved)) + '; printf "APPROVED=%s\\n" 263'
      await remote.send({'type': 'input', 'data': line})
      waiting = await remote.event('command_waiting_approval')
      queued = await host.event('command_request')
      assert queued['id'] == waiting['id'] and queued['line'] == line and not approved.exists()
      code, outcome = await request('POST', base + '/api/support_terminal/commands/' + waiting['id'] + '/approve', {})
      assert code == 200 and outcome == {'ok': True}
      assert (await remote.event('command_approved'))['id'] == waiting['id']
      assert (await remote.event('command_running'))['id'] == waiting['id']
      await remote.output_until(b'APPROVED=263\r\n')
      assert approved.read_text() == '263-approved'
      results['approval_runs_real_shell_once'] = True
      rejected = output / 'rejected.txt'
      await remote.send({'type': 'input', 'data': 'touch ' + shlex.quote(str(rejected))})
      pending = await remote.event('command_waiting_approval')
      assert (await host.event('command_request'))['id'] == pending['id']
      code, outcome = await request('POST', base + '/api/support_terminal/commands/' + pending['id'] + '/reject', {})
      assert code == 200 and outcome == {'ok': True}
      assert (await remote.event('command_rejected'))['id'] == pending['id']
      code, outcome = await request('POST', base + '/api/support_terminal/commands/' + pending['id'] + '/approve', {})
      assert code == 400 and outcome == {'ok': False, 'error': 'command not pending'} and not rejected.exists()
      results['rejection_prevents_shell_effect'] = True
      expired = output / 'expired.txt'
      await remote.send({'type': 'input', 'data': 'touch ' + shlex.quote(str(expired))})
      pending = await remote.event('command_waiting_approval')
      assert (await host.event('command_request'))['id'] == pending['id']
      began = time.monotonic()
      assert (await remote.event('command_expired', timeout=20))['id'] == pending['id']
      elapsed = time.monotonic() - began
      assert 14 <= elapsed <= 18 and not expired.exists(), elapsed
      code, outcome = await request('POST', base + '/api/support_terminal/commands/' + pending['id'] + '/approve', {})
      assert code == 400 and outcome['error'] == 'command not pending'
      results['fifteen_second_expiry_prevents_shell_effect'] = True
      save(output / 'expiry.json', {'seconds': elapsed, 'expired_file_exists': expired.exists()})
      await stop([remote, host], guest_port)
      results['stop_closes_peers_and_guest_listener'] = True

      session, guest_port, pin = await start('allow_all')
      host = await owner()
      controller = await guest(guest_port, session['id'], pin, True)
      viewer = await guest(guest_port, session['id'], pin, False)
      await viewer.send({'type': 'input', 'data': 'touch ' + shlex.quote(str(output / 'viewer-denied'))})
      assert await viewer.event('input_denied') == {'type': 'input_denied', 'reason': 'viewer only'}
      await viewer.send({'type': 'raw', 'data': 'ignored'})
      assert await viewer.event('input_denied') == {'type': 'input_denied', 'reason': 'viewer only'}
      assert not (output / 'viewer-denied').exists()
      automatic = output / 'automatic.txt'
      await controller.send({'type': 'input', 'data': 'printf "263-auto" > ' + shlex.quote(str(automatic)) + '; printf "AUTO=%s\\n" 263'})
      approved_event = await controller.event('command_approved')
      assert (await host.event('command_auto_run'))['id'] == approved_event['id']
      await controller.output_until(b'AUTO=263\r\n')
      assert automatic.read_text() == '263-auto'
      await controller.close()
      peers.remove(controller)
      role = await viewer.event('control_role')
      while not role['granted']:
        role = await viewer.event('control_role')
      await viewer.send({'type': 'raw', 'data': 'printf "HANDOFF=%s\\n" 263\r'})
      await viewer.output_until(b'HANDOFF=263\r\n')
      results['allow_all_controller_viewer_and_handoff'] = True
      await host.close()
      peers.remove(host)
      assert (await viewer.event('owner_presence'))['active'] is False
      await viewer.send({'type': 'input', 'data': 'touch ' + shlex.quote(str(denied))})
      assert await viewer.event('owner_absent') == {'type': 'owner_absent'}
      assert not denied.exists()
      results['owner_disconnect_revokes_execution'] = True
      await stop([viewer], guest_port)
      save(output / 'static.json', static)
      save(output / 'result.json', results)
      return {'scenarios': results, 'static': static}
    finally:
      save(output / 'wire.json', wire_rows)
      save(output / 'http.json', http_rows)
      save(output / 'webhooks.json', webhook_rows)
      for peer in peers:
        await peer.stream.aclose()


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  root.mkdir(parents=True, exist_ok=True)
  fixture_binary = (args.fixture_binary or args.binary.resolve().parent / 'examples/recovery_actions').resolve()
  outcomes = []
  for provider in ['source', 'native']:
    output = root / provider
    output.mkdir(exist_ok=True)
    shell = output / 'fixture-shell'
    shell.write_text('#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n')
    shell.chmod(0o700)
    webhook = Webhook()
    with socket.socket() as listener:
      listener.bind(('127.0.0.1', 0))
      port = listener.getsockname()[1]
    params = output / 'fixture-params'
    (params / 'alias').mkdir(parents=True, exist_ok=True)
    if not (params / 'd').exists():
      (params / 'd').symlink_to('alias', target_is_directory=True)
    for key, value in {'CarName': 'owned-support-car', 'DongleId': 'owned-support-id', 'HardwareSerial': 'owned-support-serial'}.items():
      (params / 'd' / key).write_text(value)
    (output / 'openpilot/selfdrive/carrot').mkdir(parents=True, exist_ok=True)
    web = output / 'openpilot/selfdrive/carrot/web'
    if not web.exists():
      web.symlink_to(Path.cwd() / 'openpilot/selfdrive/carrot/web', target_is_directory=True)
    command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_recovery_support_source.py')), '--port', str(port), '--output', str(output)] if provider == 'source' else [str(fixture_binary), str(output), str(output / 'tmux.log'), str(args.binary.resolve().with_name('openpilot-process-child')), str(port)]
    environment = {**os.environ, 'OPENPILOT_ROOT': str(output), 'CARROT_DATA_DIR': str(output / 'fixture-data'), 'USER': 'comma', 'SHELL': str(shell), 'PS1': 'OWNED> ', 'PS2': 'CONT> ', 'CARROT_SUPPORT_FAKE_TUNNEL_URL': 'https://owned-support.invalid', 'CARROT_SUPPORT_DISCORD_WEBHOOK_URL': webhook.url, 'CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE': '0'}
    with (output / 'process.log').open('wb') as log:
      process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, env=environment)
      save(output / 'invocation.json', {'command': command, 'pid': process.pid, 'binary_sha256': hashlib.sha256(fixture_binary.read_bytes()).hexdigest() if provider == 'native' else None, 'owned_params': str(params), 'owned_data': environment['CARROT_DATA_DIR'], 'owned_webhook': webhook.url, 'source_supported_fake_tunnel_url': environment['CARROT_SUPPORT_FAKE_TUNNEL_URL']})
      try:
        with anyio.fail_after(5):
          while True:
            try:
              probe = await anyio.connect_tcp('127.0.0.1', port)
              await probe.aclose()
              break
            except OSError:
              await anyio.sleep(0.01)
        outcomes.append(await exercise(port, output, webhook))
      finally:
        if provider == 'source':
          await process.stdin.send(b'\n')
          await process.stdin.aclose()
        else:
          process.send_signal(signal.SIGTERM)
        with anyio.fail_after(8):
          await process.wait()
        save(output / 'cleanup.json', {'exit': process.returncode})
        assert process.returncode == 0
        await process.aclose()
        await webhook.close()
  assert outcomes[0] == outcomes[1], outcomes
  save(root / 'result.json', {'source_native_equal': True, **outcomes[0], 'scope': 'owned loopback peers, source-supported fake tunnel, owned webhook, no-profile bash provider; no live recipient or device'})
  print('PASS recovery support source/native')


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture-binary', type=Path, help='Owned recovery_actions example; defaults to examples/recovery_actions beside --binary')
  parser.add_argument('--output', type=Path, required=True)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)

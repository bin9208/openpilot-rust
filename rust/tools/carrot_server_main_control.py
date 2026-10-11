from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time
import uuid

import anyio
from aiohttp import ClientSession, web
from carrot_server_dashcam_upload import save
from carrot_server_main_receiver import held_receiver


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  root.mkdir(parents=True, exist_ok=True)
  namespace = 'rust-probe-main-' + uuid.uuid4().hex
  queue = Path('/dev/shm') / ('msgq_' + namespace)
  queue.mkdir()
  params = root / 'params' / namespace
  params.mkdir(parents=True)
  for key, value in [('IsOffroad', b'1'), ('IsOnroad', b'0'), ('Version', b'owned-main-version'), ('GithubUsername', b'owned-main-owner')]:
    (params / key).write_bytes(value)
  programs = root / 'bin'
  programs.mkdir()
  program = programs / 'nmcli'
  program.write_text('#!/bin/sh\nexit 0\n')
  program.chmod(0o755)
  env = {**os.environ, 'USER': 'comma', 'LOGNAME': 'comma', 'HOME': str(root), 'OPENPILOT_ROOT': '/data/openpilot', 'PARAMS_ROOT': str(root / 'params'), 'OPENPILOT_PREFIX': namespace, 'CARROT_DATA_DIR': str(root / 'data'), 'CARROT_REPO_LOCK_PATH': str(root / 'repository.lock'), 'PATH': str(programs) + ':' + os.environ['PATH'], 'NO_PROXY': '*'}
  subprocess.run(['/usr/sbin/ip', 'link', 'set', 'lo', 'up'], check=True)
  with socket.socket() as listener:
    listener.bind(('127.0.0.1', 0))
    port = listener.getsockname()[1]
  command = [str(args.owner.resolve()), str(root), '/data/openpilot/rust/target/debug/openpilot-process-child', str(args.binary.resolve()), str(port)] if args.mode != 'term' else [str(args.binary.resolve()), '--host', '127.0.0.1', '--port', str(port)]
  receiver = await held_receiver(root, args.mode == 'force')
  log = (root / 'process.log').open('wb')
  process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, env=env)
  observed = []
  readiness_errors = []
  pty_pid = None
  try:
    pid = process.pid
    if args.mode != 'term':
      with anyio.fail_after(5):
        while not (root / 'owner-start.json').exists():
          await anyio.sleep(0.01)
      pid = json.loads((root / 'owner-start.json').read_text())['pid']
    async with ClientSession() as client:
      url = f'http://127.0.0.1:{port}'
      with anyio.fail_after(8):
        while True:
          try:
            async with client.get(url + '/api/heartbeat_status') as response:
              if response.status == 200:
                break
          except OSError as error:
            readiness_errors.append({'type': type(error).__name__, 'message': str(error)})
          await anyio.sleep(0.02)
      digest = hashlib.sha256(Path(f'/proc/{pid}/exe').read_bytes()).hexdigest()
      assert digest == hashlib.sha256(args.binary.read_bytes()).hexdigest()
      save(root / 'invocation.json', {'command': command, 'pid': pid, 'executable_sha256': digest, 'network_namespace': os.readlink('/proc/self/ns/net'), 'route_table': Path('/proc/net/route').read_text(), 'data': env['CARROT_DATA_DIR'], 'uid': os.getuid(), 'login_user_environment': env['USER'], 'readiness_errors': readiness_errors, 'scope': 'normal native main executable in owned filesystem/network namespaces; USER=comma selects the source login branch under the namespace UID'})
      for path in ['/', '/api/settings', '/api/params_bulk?names=IsOffroad,IsOnroad', '/api/live_runtime', '/api/terminal_pty/status', '/api/support_terminal/status', '/api/terminal_commands', '/api/tools/jobs', '/api/vision_test/status', '/api/vision_diag/server_snapshot']:
        async with client.get(url + path) as response:
          body = await response.read()
          observed.append({'path': path, 'status': response.status, 'content_type': response.headers.get('Content-Type'), 'bytes': len(body), 'sha256': hashlib.sha256(body).hexdigest(), 'body': body.decode() if path != '/' else body[:160].decode()})
          save(root / 'http.json', observed)
          assert response.status == 200
      raw = await client.ws_connect(url + '/ws/raw/carState')
      hello = await raw.receive_json()
      assert hello['service'] == 'carState'
      await raw.close()
      if args.mode != 'force':
        terminal = await client.ws_connect(url + '/ws/terminal_pty')
        terminal_hello = await terminal.receive_json()
        pty_pid = terminal_hello['pid']
        await terminal.send_json({'type': 'input', 'data': "printf 'owned-main-pty-result\\n'"})
        output = ''
        with anyio.fail_after(4):
          while 'owned-main-pty-result\r\n' not in output:
            message = await terminal.receive_json()
            output += base64.b64decode(message.get('b64', '')).decode(errors='replace')
        save(root / 'terminal.json', {'hello': terminal_hello, 'output': output, 'real_pty_pid': pty_pid})
        await terminal.close()
      else:
        with anyio.fail_after(5):
          await receiver['received'].wait()
      began = time.monotonic()
      if args.mode == 'term':
        process.send_signal(signal.SIGTERM)
      else:
        await process.stdin.send(b'stop\n')
        await process.stdin.aclose()
      with anyio.fail_after(8):
        await process.wait()
      elapsed = time.monotonic() - began
      owner = json.loads((root / 'owner-stop.json').read_text()) if args.mode != 'term' else None
      if args.mode == 'force':
        assert owner['exit'] == -9 and 5 <= owner['seconds'] < 6
        assert len(receiver['chunks']) >= 5
      else:
        assert process.returncode == 0 and elapsed < 5
        if owner:
          assert owner['exit'] == 0
      assert not Path(f'/proc/{pid}').exists() and (pty_pid is None or not Path(f'/proc/{pty_pid}').exists())
      save(root / 'result.json', {'mode': args.mode, 'normal_entrypoint': True, 'http_routes': len(observed), 'raw_websocket': hello, 'pty_reaped': pty_pid is not None, 'seconds': elapsed, 'owner': owner, 'heartbeat_chunks': receiver['chunks'], 'process_exit': process.returncode, 'main_reaped': True})
  finally:
    if process.returncode is None:
      process.kill()
      await process.wait()
    await process.aclose()
    await receiver['runner'].cleanup()
    log.close()
    import shutil
    shutil.rmtree(queue)
  print('PASS')


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'owner', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--mode', choices=['term', 'supervisor', 'force'], required=True)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)

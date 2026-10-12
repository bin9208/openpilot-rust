from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import signal
import socket
import subprocess
import time

import anyio
from aiohttp import ClientError, ClientSession
from carrot_recovery_terminal_control import save
from carrot_recovery_terminal_lifecycle import state


async def run(args: argparse.Namespace) -> None:
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  tunnel = output / 'cloudflared'
  tunnel.write_text('#!/bin/sh\nprintf "%s\\n" "$$" > "$CARROT_OWNED_TUNNEL_ENTERED"\nwhile ! test -f "$CARROT_OWNED_TUNNEL_RELEASE"; do sleep 0.1; done\nprintf "https://owned-fixture.trycloudflare.com\\n"\nexec sleep 600\n')
  tunnel.chmod(0o700)
  shell = output / 'fixture-shell'
  shell.write_text('#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n')
  shell.chmod(0o700)
  with socket.socket() as listener:
    listener.bind(('127.0.0.1', 0))
    port = listener.getsockname()[1]
  command = [str(args.binary.resolve()), '--host', '127.0.0.1', '--port', str(port)]
  environment = {key: value for key, value in os.environ.items() if key != 'CARROT_SUPPORT_FAKE_TUNNEL_URL'}
  environment.update({'OPENPILOT_ROOT': str(Path.cwd()), 'USER': 'comma', 'SHELL': str(shell), 'CARROT_CLOUDFLARED_BIN': str(tunnel), 'CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE': '1', 'CARROT_OWNED_TUNNEL_ENTERED': str(output / 'entered'), 'CARROT_OWNED_TUNNEL_RELEASE': str(output / 'release')})
  with (output / 'process.log').open('wb') as log:
    process = await anyio.open_process(command, stdout=log, stderr=subprocess.STDOUT, env=environment)
    save(output / 'invocation.json', {'command': command, 'pid': process.pid, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()})
    pending = {}
    try:
      async with ClientSession() as client:
        with anyio.fail_after(5):
          while True:
            try:
              async with client.get(f'http://127.0.0.1:{port}/api/support_terminal/status') as response:
                assert response.status == 200
                break
            except OSError:
              await anyio.sleep(0.01)
        async def start() -> None:
          try:
            async with client.post(f'http://127.0.0.1:{port}/api/support_terminal/start', json={'note': 'owned cancellation'}) as response:
              pending.update({'status': response.status, 'body': await response.text()})
          except (OSError, ClientError) as error:
            pending.update({'closed': type(error).__name__})
        async with anyio.create_task_group() as tasks:
          tasks.start_soon(start)
          with anyio.fail_after(5):
            while not (output / 'entered').exists():
              await anyio.sleep(0.01)
          tunnel_pid = int((output / 'entered').read_text())
          began = time.monotonic()
          process.send_signal(signal.SIGTERM)
          exited = False
          with anyio.move_on_after(3):
            await process.wait()
            exited = True
          observation = {'scenario': 'SIGTERM during real pending support tunnel startup', 'tunnel_entered': True, 'tunnel_pid': tunnel_pid, 'exited_within_three_seconds': exited, 'seconds_before_release': time.monotonic() - began, 'expect_blocked': args.expect_blocked}
          save(output / 'observation.json', observation)
          if not exited:
            (output / 'release').write_text('release owned URL provider\n')
          with anyio.fail_after(8):
            await process.wait()
          tasks.cancel_scope.cancel()
        save(output / 'request.json', pending)
      save(output / 'cleanup.json', {'exit': process.returncode, 'tunnel_state_after': state(tunnel_pid)})
      assert process.returncode == 0 and state(tunnel_pid) is None
      assert exited != args.expect_blocked, observation
      save(output / 'result.json', {'expected_observable_confirmed': True, **observation})
      print('PASS owned support startup shutdown' if exited else 'RED confirmed support startup blocks shutdown')
    finally:
      if process.returncode is None:
        (output / 'release').write_text('release on fixture cleanup\n')
        process.send_signal(signal.SIGTERM)
        with anyio.fail_after(10):
          await process.wait()
      await process.aclose()


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--expect-blocked', action='store_true')
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)

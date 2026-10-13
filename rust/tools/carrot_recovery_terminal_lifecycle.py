from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import socket
import subprocess
import sys

import anyio
from carrot_recovery_terminal_control import connect, save


def state(pid: int) -> str | None:
  try:
    return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
  except FileNotFoundError:
    return None


async def terminated(pid: int) -> str | None:
  with anyio.fail_after(5):
    while True:
      observed = state(pid)
      if observed in {None, 'Z'}:
        return observed
      await anyio.sleep(0.01)


def cleanup_source_owned(marker: str) -> list:
  removed = []
  token = ('CARROT_RECOVERY_LIFECYCLE_ID=' + marker).encode()
  for process in Path('/proc').iterdir():
    if not process.name.isdecimal():
      continue
    try:
      if token not in (process / 'environ').read_bytes().split(b'\0'):
        continue
      identity = (process / 'stat').read_text()
      pid = int(process.name)
      if (process / 'stat').read_text() != identity:
        continue
      os.kill(pid, signal.SIGKILL)
      removed.append({'pid': pid, 'owned_environment_marker': marker})
    except (FileNotFoundError, ProcessLookupError, PermissionError):
      continue
  return removed


async def exercise(port: int, output: Path, native: bool) -> dict:
  rows = []
  pids = []
  peers = []
  result = {'reset_rounds': [], 'source_race_observed': False}
  try:
    for number in range(24):
      ws, _ = await connect(port, rows, reset=True)
      peers.append(ws)
      try:
        meta = await ws.event('meta')
        assert meta['created'] is True and meta['rows'] == 30
        await ws.send({'type': 'input', 'data': f'printf "ROUND=%s,{number}\\n" "$$"'})
        output_bytes = await ws.output_until(f',{number}\r\n'.encode())
        pid = int(re.search(rb'ROUND=(\d+),\d+\r\n', output_bytes)[1])
        pids.append(pid)
        result['reset_rounds'].append({'round': number, 'pid': pid, 'command_completed': True})
      except (AssertionError, anyio.EndOfStream, anyio.BrokenResourceError, TimeoutError) as error:
        save(output / 'reset-failure.json', {'round': number, 'error': str(error), 'wire_count': len(rows)})
        result['source_race_observed'] = not native
        if native:
          raise
        break
      await ws.close()
      peers.remove(ws)
    if native:
      previous, _ = await connect(port, rows, reset=True)
      peers.append(previous)
      assert (await previous.event('meta'))['created'] is True
      ws, _ = await connect(port, rows, reset=True)
      peers.append(ws)
      assert (await ws.event('meta'))['created'] is True
      assert await previous.event('pty_exit') == {'type': 'pty_exit', 'session': 'recovery-shell', 'exit_code': None}
      assert (await previous.frame())[0] == 8
      await previous.stream.aclose()
      peers.remove(previous)
      result['active_peer_closed_before_replacement'] = True
      child_script = 'trap "" HUP; sleep 600 & printf "GRANDCHILD=%s\\n" "$!"; wait'
      await ws.send({'type': 'input', 'data': '/bin/sh -c ' + shlex.quote(child_script) + ' & printf "CHILD=%s\\n" "$!"; printf "LEADER=%s\\n" "$$"'})
      captured = await ws.output_until(b'OWNED> ')
      while re.search(rb'LEADER=(\d+)\r\n', captured) is None or re.search(rb'GRANDCHILD=(\d+)\r\n', captured) is None:
        captured += base64.b64decode((await ws.event('pty_output'))['b64'])
      child = int(re.search(rb'(?:^|\r\n)CHILD=(\d+)\r\n', captured)[1])
      leader = int(re.search(rb'LEADER=(\d+)\r\n', captured)[1])
      grandchild = int(re.search(rb'GRANDCHILD=(\d+)\r\n', captured)[1])
      result['leader_exit_child'] = {'leader': leader, 'child': child, 'grandchild': grandchild, 'child_before': state(child)}
      await ws.send({'type': 'input', 'data': 'exit 9'})
      assert (await ws.event('pty_exit'))['exit_code'] == 9
      result['leader_exit_child']['child_after'] = await terminated(child)
      result['leader_exit_child']['leader_after'] = await terminated(leader)
      result['leader_exit_child']['grandchild_after'] = await terminated(grandchild)
      assert all(result['leader_exit_child'][key] is None for key in ['child_after', 'leader_after', 'grandchild_after'])
      await ws.stream.aclose()
      peers.remove(ws)
      result['old_leaders_after_reset'] = [{'pid': pid, 'state': await terminated(pid)} for pid in pids]
      assert all(row['state'] is None for row in result['old_leaders_after_reset'])
      shutdown, _ = await connect(port, rows, reset=True)
      peers.append(shutdown)
      assert (await shutdown.event('meta'))['created'] is True
      await shutdown.send({'type': 'input', 'data': '/bin/sh -c \'trap "" HUP; exec sleep 600\' & printf "SHUTDOWN_CHILD=%s\\n" "$!"; printf "SHUTDOWN_LEADER=%s\\n" "$$"'})
      captured = await shutdown.output_until(b'OWNED> ')
      while re.search(rb'SHUTDOWN_LEADER=(\d+)\r\n', captured) is None:
        captured += base64.b64decode((await shutdown.event('pty_output'))['b64'])
      result['shutdown_child'] = int(re.search(rb'SHUTDOWN_CHILD=(\d+)\r\n', captured)[1])
      result['shutdown_leader'] = int(re.search(rb'SHUTDOWN_LEADER=(\d+)\r\n', captured)[1])
    save(output / 'result.json', result)
    return result
  finally:
    save(output / 'wire.json', rows)
    for peer in peers:
      await peer.stream.aclose()


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  root.mkdir(parents=True, exist_ok=True)
  outcomes = []
  for provider in ['source', 'native']:
    output = root / provider
    output.mkdir(exist_ok=True)
    shell = output / 'fixture-shell'
    shell.write_text('#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n')
    shell.chmod(0o700)
    with socket.socket() as listener:
      listener.bind(('127.0.0.1', 0))
      port = listener.getsockname()[1]
    command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_recovery_source.py')), '--port', str(port), '--output', str(output)] if provider == 'source' else [str(args.binary.resolve()), '--host', '127.0.0.1', '--port', str(port)]
    environment = {**os.environ, 'OPENPILOT_ROOT': str(Path.cwd()), 'USER': 'comma', 'SHELL': str(shell), 'PS1': 'OWNED> ', 'PS2': 'CONT> ', 'CARROT_RECOVERY_LIFECYCLE_ID': str(output)}
    with (output / 'process.log').open('wb') as log:
      process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, env=environment)
      save(output / 'invocation.json', {'command': command, 'pid': process.pid, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if provider == 'native' else None})
      outcome = None
      try:
        with anyio.fail_after(5):
          while True:
            try:
              ready = await anyio.connect_tcp('127.0.0.1', port)
              await ready.aclose()
              break
            except OSError:
              await anyio.sleep(0.01)
        outcome = await exercise(port, output, provider == 'native')
        outcomes.append(outcome)
      finally:
        if provider == 'source':
          await process.stdin.send(b'\n')
          await process.stdin.aclose()
        else:
          process.send_signal(signal.SIGTERM)
        with anyio.fail_after(5):
          await process.wait()
        cleanup = {'exit_code': process.returncode}
        if provider == 'source':
          cleanup['source_fixture_leftovers_removed'] = cleanup_source_owned(str(output))
        if provider == 'native' and outcome:
          cleanup['shutdown_child_after'] = await terminated(outcome['shutdown_child'])
          cleanup['shutdown_leader_after'] = await terminated(outcome['shutdown_leader'])
          assert cleanup['shutdown_child_after'] is None and cleanup['shutdown_leader_after'] is None
        save(output / 'cleanup.json', cleanup)
        assert process.returncode == 0
        await process.aclose()
  assert len(outcomes[1]['reset_rounds']) == 24 and not outcomes[1]['source_race_observed']
  save(root / 'result.json', {'native_24_reset_rounds': True, 'native_active_peer_closed_before_replacement': outcomes[1]['active_peer_closed_before_replacement'], 'native_leader_and_two_level_descendant_cleanup': True, 'native_server_shutdown_cleanup': True, 'source_race_observed': outcomes[0]['source_race_observed'], 'source_successful_rounds': len(outcomes[0]['reset_rounds']), 'limits': ['source reset has generation-race risk by source inspection; this run only reports whether it reproduced', 'native recovery enables child-subreaper and reaps only owned-session adopted children; no mandatory pidfd syscall']})
  print('PASS native recovery terminal lifecycle')


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)

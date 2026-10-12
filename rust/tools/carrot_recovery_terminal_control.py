from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import sys
import time

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream


def save(path: Path, value: object) -> None:
  path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def wire(payload: bytes, opcode: int = 1, fin: bool = True, masked: bool = True) -> bytes:
  header = bytes([opcode | (128 if fin else 0)])
  flag = 128 if masked else 0
  size = len(payload)
  if size < 126:
    header += bytes([flag | size])
  elif size < 65536:
    header += bytes([flag | 126]) + struct.pack('!H', size)
  else:
    header += bytes([flag | 127]) + struct.pack('!Q', size)
  mask = b'\x03\x07\x0b\x13'
  return header + (mask + bytes(byte ^ mask[index & 3] for index, byte in enumerate(payload)) if masked else payload)


@dataclass
class Ws:
  stream: anyio.abc.ByteStream
  reader: BufferedByteReceiveStream
  rows: list

  async def frame(self, timeout: float = 8) -> tuple[int, bytes]:
    with anyio.fail_after(timeout):
      header = await self.reader.receive_exactly(2)
      length = header[1] & 127
      match length:
        case 126:
          length = struct.unpack('!H', await self.reader.receive_exactly(2))[0]
        case 127:
          length = struct.unpack('!Q', await self.reader.receive_exactly(8))[0]
      assert header[0] & 128 and not header[1] & 128
      payload = await self.reader.receive_exactly(length) if length else b''
    self.rows.append({'opcode': header[0] & 15, 'payload_base64': base64.b64encode(payload).decode()})
    return header[0] & 15, payload

  async def send(self, value: object) -> None:
    await self.stream.send(wire(json.dumps(value).encode()))

  async def event(self, typ: str, timeout: float = 8) -> dict:
    while True:
      opcode, payload = await self.frame(timeout)
      assert opcode != 8, ('closed before event', typ, self.rows)
      if opcode == 1:
        data = json.loads(payload)
        if data.get('type') == typ:
          return data

  async def output_until(self, marker: bytes) -> bytes:
    output = bytearray()
    while marker not in output:
      event = await self.event('pty_output')
      output.extend(base64.b64decode(event['b64']))
    return bytes(output)

  async def close(self) -> None:
    await self.stream.send(wire(b'', 8))
    await self.stream.aclose()


async def connect(port: int, rows: list, reset: bool = False, initial: bytes = b'', path: str = '/ws/terminal_pty') -> tuple[Ws, bytes]:
  stream = await anyio.connect_tcp('127.0.0.1', port)
  reader = BufferedByteReceiveStream(stream)
  request = ('GET ' + path + ('?reset=1' if reset else '') + ' HTTP/1.1\r\nHost: owned\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n').encode()
  await stream.send(request + initial)
  with anyio.fail_after(5):
    handshake = await reader.receive_until(b'\r\n\r\n', 65536)
  rows.append({'handshake': handshake.decode(errors='replace')})
  return Ws(stream, reader, rows), handshake


async def scenario(port: int, output: Path) -> dict:
  rows = []
  peers = []
  shell_pid = None
  result = {}
  try:
    ws, handshake = await connect(port, rows, initial=wire(json.dumps({'type': 'input', 'data': 'printf "PREBUFFER=%s\\n" 263'}).encode()))
    peers.append(ws)
    assert handshake.startswith(b'HTTP/1.1 101 '), handshake
    assert b'Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=' in handshake
    meta = await ws.event('meta')
    assert meta == {'type': 'meta', 'mode': 'pty', 'session': 'recovery-shell', 'created': True, 'user': 'comma', 'rows': 30, 'cols': 100}, meta
    captured = await ws.output_until(b'PREBUFFER=263\r\n')
    await ws.send({'type': 'input', 'data': 'printf "PID=%s\\n" "$$"; printf "ENV=%s,%s,%s\\n" "$TERM" "$COLORTERM" "${TMUX-unset}"; stty size; printf "CWD=%s\\n" "$PWD"'})
    captured += await ws.output_until(b'OWNED> ')
    expected_cwd = ('CWD=' + str(Path.cwd() / 'openpilot') + '\r\n').encode()
    while b'PID=' not in captured or b'ENV=xterm-256color,truecolor,unset\r\n' not in captured or b'30 100\r\n' not in captured or expected_cwd not in captured:
      captured += await ws.output_until(b'OWNED> ')
    import re
    shell_pid = int(re.search(rb'PID=(\d+)\r\n', captured)[1])
    result['meta'] = meta
    result['pid'] = shell_pid
    result['environment'] = 'xterm-256color,truecolor,unset'
    result['initial_geometry'] = [30, 100]
    result['source_repository_cwd'] = True
    result['prebuffered_frame'] = True

    fragment = json.dumps({'type': 'input', 'data': 'printf "FRAG=%s\\n" 263'}).encode()
    await ws.stream.send(wire(fragment[:20], 1, False) + wire(b'probe263', 9) + wire(b'ignored', 10) + wire(fragment[20:], 0))
    while True:
      opcode, payload = await ws.frame()
      if opcode == 10:
        assert payload == b'probe263'
        break
    await ws.output_until(b'FRAG=263\r\n')
    result['fragmented_ping_pong'] = True
    await ws.stream.send(wire(json.dumps({'type': 'raw', 'data': 'printf "RAW=%s\\n" 263\r'}).encode(), 2, masked=False))
    await ws.output_until(b'RAW=263\r\n')
    result['unmasked_binary_raw'] = True
    for action, expected in [('ctrl_c', [3]), ('refresh', [12]), ('detach', [96, 100])]:
      ready = 'CONTROL_READY_' + action
      done = 'CONTROL_DONE_' + action
      command = 'stty -icanon -echo -isig min 1 time 0; printf "' + ready + '=%s\\n" 263; od -An -tu1 -N' + str(len(expected)) + '; stty sane; printf "' + done + '=%s\\n" 263'
      await ws.send({'type': 'input', 'data': command})
      await ws.output_until((ready + '=263\r\n').encode())
      await ws.send({'type': 'control', 'action': action})
      observed = await ws.output_until((done + '=263\r\n').encode())
      byte_rows = re.findall(rb'(?:^|\r\n)\s*(\d+(?:\s+\d+)*)\r\n', observed)
      assert any([int(value) for value in row.split()] == expected for row in byte_rows), (action, observed)
      result['control_' + action + '_pty_bytes'] = expected
    await ws.send({'type': 'resize', 'rows': 222, 'cols': 500})
    resize = await ws.event('pty_resize')
    assert resize == {'type': 'pty_resize', 'session': 'recovery-shell', 'rows': 200, 'cols': 100}, resize
    await ws.send({'type': 'input', 'data': 'stty size; printf "RESIZED=%s\\n" 263'})
    resized = await ws.output_until(b'RESIZED=263\r\n')
    assert b'200 100\r\n' in resized
    result['resize'] = resize

    await ws.send({'type': 'input', 'data': 'persistent_value=263; printf "REMEMBER=%s\\n" "$persistent_value"'})
    await ws.output_until(b'REMEMBER=263\r\n')
    await ws.close()
    peers.remove(ws)
    ws, _ = await connect(port, rows)
    peers.append(ws)
    reconnect = await ws.event('meta')
    replay = await ws.event('pty_output')
    assert reconnect['created'] is False and reconnect['rows'] == 200
    assert replay.get('replay') is True and b'REMEMBER=263\r\n' in base64.b64decode(replay['b64'])
    await ws.send({'type': 'input', 'data': 'printf "PERSIST=%s,%s\\n" "$$" "$persistent_value"'})
    await ws.output_until(f'PERSIST={shell_pid},263\r\n'.encode())
    result['reconnect_persistent_pid_and_variable'] = True
    result['history_replay'] = True

    await ws.send({'type': 'input', 'data': 'printf "OLD_HISTORY=%s\\n" 263'})
    await ws.output_until(b'OLD_HISTORY=263\r\n')
    await ws.send({'type': 'control', 'action': 'clear'})
    await ws.send({'type': 'input', 'data': 'printf "AFTER_CLEAR=%s\\n" 263'})
    await ws.output_until(b'AFTER_CLEAR=263\r\n')
    second, _ = await connect(port, rows)
    peers.append(second)
    assert (await second.event('meta'))['created'] is False
    cleared = await second.event('pty_output')
    cleared_bytes = base64.b64decode(cleared['b64'])
    assert cleared.get('replay') is True and b'OLD_HISTORY=263\r\n' not in cleared_bytes and b'AFTER_CLEAR=263\r\n' in cleared_bytes
    result['clear_history'] = True
    await ws.send({'type': 'input', 'data': 'printf "BROADCAST=%s\\n" 263'})
    await ws.output_until(b'BROADCAST=263\r\n')
    await second.output_until(b'BROADCAST=263\r\n')
    result['two_client_broadcast'] = True

    await ws.send({'type': 'input', 'data': 'head -c 600000 /dev/zero | tr "\\0" X; printf "BIG_END=%s\\n" 263'})
    await ws.output_until(b'BIG_END=263\r\n')
    large, _ = await connect(port, rows)
    peers.append(large)
    assert (await large.event('meta'))['created'] is False
    bounded = await large.event('pty_output')
    bounded_bytes = base64.b64decode(bounded['b64'])
    assert bounded.get('replay') is True and len(bounded_bytes) == 512 * 1024
    assert b'BIG_END=263\r\n' in bounded_bytes and b'BROADCAST=263\r\n' not in bounded_bytes
    (output / 'bounded-history.bin').write_bytes(bounded_bytes)
    result['bounded_history_bytes'] = len(bounded_bytes)

    await ws.send({'type': 'input', 'data': 'exit 7'})
    exits = [await peer.event('pty_exit') for peer in peers]
    assert all(row['exit_code'] == 7 for row in exits), exits
    for peer in peers:
      opcode, _ = await peer.frame()
      assert opcode == 8
      await peer.stream.aclose()
    peers.clear()
    result['exit_code_and_client_close'] = 7
    fresh, _ = await connect(port, rows, reset=True)
    peers.append(fresh)
    recreated = await fresh.event('meta')
    assert recreated['created'] is True and recreated['rows'] == 30
    await fresh.send({'type': 'input', 'data': 'printf "NEW=%s,%s\\n" "$$" "${persistent_value-unset}"'})
    new_output = await fresh.output_until(b',unset\r\n')
    new_pid = int(re.search(rb'NEW=(\d+),unset\r\n', new_output)[1])
    assert new_pid != shell_pid
    result['new_shell_after_exit'] = True
    result['final_pid'] = new_pid
    await fresh.send({'type': 'input', 'data': 'exit 0'})
    assert (await fresh.event('pty_exit'))['exit_code'] == 0
    await fresh.stream.aclose()
    peers.clear()
    save(output / 'result.json', result)
    return result
  finally:
    save(output / 'wire.json', rows)
    for peer in peers:
      await peer.stream.aclose()


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  root.mkdir(parents=True, exist_ok=True)
  results = []
  for provider in ['source', 'native']:
    output = root / provider
    output.mkdir(exist_ok=True)
    shell = output / 'fixture-shell'
    shell.write_text('#!/bin/sh\nprintf "%s\\n" "$@" >> "$CARROT_RECOVERY_SHELL_LOG"\nexec /bin/bash --noprofile --norc -i\n')
    shell.chmod(0o700)
    with socket.socket() as listener:
      listener.bind(('127.0.0.1', 0))
      port = listener.getsockname()[1]
    command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_recovery_source.py')), '--port', str(port), '--output', str(output)] if provider == 'source' else [str(args.binary.resolve()), '--host', '127.0.0.1', '--port', str(port)]
    environment = {key: value for key, value in os.environ.items() if key not in {'TERM', 'COLORTERM'}}
    environment.update({'OPENPILOT_ROOT': str(Path.cwd()), 'USER': 'comma', 'SHELL': str(shell), 'PS1': 'OWNED> ', 'PS2': 'CONT> ', 'TMUX': 'must-be-removed', 'CARROT_RECOVERY_SHELL_LOG': str(output / 'shell-argv.log')})
    with (output / 'process.log').open('wb') as log:
      process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, env=environment)
      save(output / 'invocation.json', {'command': command, 'pid': process.pid, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if provider == 'native' else None, 'shell_provider': 'owned no-profile bash, original bootstrap captured but not executed', 'user_policy_branch': 'USER=comma'})
      try:
        with anyio.fail_after(5):
          while True:
            try:
              stream = await anyio.connect_tcp('127.0.0.1', port)
              await stream.aclose()
              break
            except OSError:
              await anyio.sleep(0.01)
        results.append(await scenario(port, output))
      finally:
        if provider == 'source':
          await process.stdin.send(b'\n')
          await process.stdin.aclose()
        else:
          process.send_signal(signal.SIGTERM)
        with anyio.fail_after(5):
          await process.wait()
        save(output / 'cleanup.json', {'exit_code': process.returncode})
        assert process.returncode == 0
        await process.aclose()
  for row in results:
    del row['pid']
    del row['final_pid']
  assert results[0] == results[1], results
  assert (root / 'source/shell-argv.log').read_text() == (root / 'native/shell-argv.log').read_text()
  save(root / 'result.json', {'source_native_equal': True, 'scenarios': results[0], 'shell_bootstrap_argv_equal': True, 'limits': ['owned SHELL provider runs bash without login files; real device login/MOTD and sudo/su user switching remain target behavior requiring user comparison', 'native reset closes prior-generation clients before replacement, repairing source stale-reader race', 'native PTY and socket writes have bounded two-second deadlines; source writes can block indefinitely']})
  print('PASS recovery terminal source/native')


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)

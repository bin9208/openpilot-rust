#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import pty
import select
import serial
import subprocess
import termios

from pigeon_source import load


def ready(fd):
  assert select.select([fd], [], [], 5)[0], 'PTY/child timeout'


def attributes(fd):
  values = termios.tcgetattr(fd)
  return values[:6] + [[ord(value) if isinstance(value, bytes) else value for value in values[6]]]


def exercise(binary):
  master, slave = pty.openpty()
  process = None
  pigeon = None
  try:
    path = os.ttyname(slave)
    if binary:
      process = subprocess.Popen([str(binary), path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
      ready(process.stdout)
      assert json.loads(process.stdout.readline()) == {'ready': True}

      def command(action):
        process.stdin.write(action + '\n')
        process.stdin.flush()
        ready(process.stdout)
        return json.loads(process.stdout.readline())
    else:
      source = load()
      source.update(serial=serial, UBLOX_TTY=path)
      pigeon = source['TTYPigeon']()

      def command(action):
        match action:
          case 'read':
            return {'data': list(pigeon.receive())}
          case 'baud':
            pigeon.set_baud(460800)
            return {'baud': 460800}
          case 'send':
            pigeon.send(b'\x00\xb5\x62\xff\r\n')
            return {'sent': True}
          case _:
            raise AssertionError(action)

    initial = attributes(slave)
    empty = command('read')
    sent = bytes(range(256))
    os.write(master, sent)
    ready(slave)
    received = command('read')
    assert received == {'data': list(sent)}
    write = command('send')
    ready(master)
    output = os.read(master, 64)
    baud = command('baud')
    final = attributes(slave)
    return {'initial': initial, 'empty': empty, 'received': received, 'write': write, 'output': list(output), 'baud': baud, 'final': final}
  finally:
    if process:
      process.stdin.close()
      assert process.wait(timeout=5) == 0, process.stderr.read()
    if pigeon:
      pigeon.tty.close()
    os.close(master)
    os.close(slave)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source, native = exercise(None), exercise(args.binary.resolve())
  for name, result in [('source', source), ('native', native)]:
    (args.evidence / f'serial-{name}.json').write_text(json.dumps(result, indent=2) + '\n')
  assert source == native, 'PTY source/native mismatch'
  root = Path(__file__).resolve().parents[1] / 'crates/ublox/native'
  binary = (args.evidence / 'serial-sanitizer').resolve()
  command = [
    'clang++',
    '-std=c++17',
    '-fsanitize=address,undefined',
    '-fno-omit-frame-pointer',
    '-g',
    str(root / 'serial.cc'),
    str(root / 'serial_fixture.cc'),
    '-o',
    str(binary),
  ]
  result = subprocess.run(command, capture_output=True, text=True)
  (args.evidence / 'serial-sanitizer-build.log').write_text('COMMAND ' + repr(command) + '\n' + result.stdout + result.stderr + f'\nEXIT {result.returncode}\n')
  result.check_returncode()
  result = subprocess.run(
    [binary], capture_output=True, text=True, env=os.environ | {'ASAN_OPTIONS': 'detect_leaks=1:halt_on_error=1', 'UBSAN_OPTIONS': 'halt_on_error=1'}
  )
  (args.evidence / 'serial-sanitizer.log').write_text(result.stdout + result.stderr + f'\nEXIT {result.returncode}\n')
  result.check_returncode()
  result = {'source_native_pty_equal': True, 'asan_ubsan_passed': True, 'physical_devices_accessed': False}
  (args.evidence / 'serial-results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp", "inputs==0.5"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import pty
import signal
import struct
import subprocess
import sys
import termios
import time
import types
import uuid

from joystickd_source import load_binding


def bits(values) -> list[int]:
  return [struct.unpack('=I', struct.pack('=f', value))[0] for value in values]


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  load_binding(args.binding.resolve())
  from openpilot.cereal import messaging
  from openpilot.tools.joystick import joystick_control as original
  original.KBHit = lambda: types.SimpleNamespace(getch=lambda: keyboard_value[0])
  results = []
  binary = str(args.binary.resolve())
  wrapper = Path(__file__).with_name('joystick_input_runtime_source.py')

  for implementation in ['source', 'rust']:
    for mode in ['keyboard', 'gamepad', 'gamepad-error', 'managed-gamepad']:
      directory = args.output / (implementation + '-' + mode)
      directory.mkdir()
      prefix = 'joystick_input_' + uuid.uuid4().hex
      os.environ['OPENPILOT_PREFIX'] = prefix
      os.environ['PARAMS_ROOT'] = str((directory / 'params').resolve())
      os.environ.pop('ZMQ', None)
      params = Path(os.environ['PARAMS_ROOT']) / prefix
      params.mkdir(parents=True)
      (params / 'IsOffroad').write_text('1')
      if mode == 'keyboard':
        (params / 'IsOffroad').write_text('0')
        os.environ['ZMQ'] = ''
      if mode == 'managed-gamepad':
        (params / 'IsOffroad').write_text('0')
      if mode == 'gamepad-error':
        (params / 'JoystickDebugMode').mkdir()
      ipc = Path('/dev/shm') / ('msgq_' + prefix)
      ipc.mkdir()
      subscriber = messaging.sub_sock('testJoystick', timeout=100)
      master, slave = pty.openpty()
      normal = termios.tcgetattr(slave)
      path = directory / 'owned-event-stream'
      writer = None
      if mode in ['gamepad', 'managed-gamepad']:
        os.mkfifo(path)
      elif mode == 'gamepad-error':
        path.mkdir()
      if implementation == 'source':
        command = [sys.executable, '-P', str(wrapper), '--binding', str(args.binding.resolve())]
        command += ['--keyboard'] if mode == 'keyboard' else ['--input', str(path.resolve())]
      else:
        command = [binary, '--keyboard'] if mode == 'keyboard' else [str(args.fixture.resolve()), str(path.resolve())]
      if mode == 'managed-gamepad':
        command += ['--managed']
      stdout = (directory / 'stdout.log').open('w')
      stderr = (directory / 'stderr.log').open('w')
      child = subprocess.Popen(command, stdin=slave, stdout=stdout, stderr=stderr)
      publications, phases = [], []

      def receive():
        deadline = time.monotonic() + 5
        while True:
          message = messaging.recv_one_or_none(subscriber)
          if message is not None:
            assert message.valid and message.logMonoTime > 0
            assert list(message.testJoystick.buttons) == []
            publications.append(message.to_dict())
            return bits(message.testJoystick.axes)
          assert time.monotonic() < deadline and child.poll() is None, (command, 'publication timeout')
          time.sleep(.001)

      def match(name: str, expected: list[int]) -> None:
        consecutive = 0
        for _ in range(100):
          actual = receive()
          consecutive = consecutive + 1 if actual == expected else 0
          if consecutive == 3:
            phases.append({'name': name, 'axes_bits': actual, 'publication': len(publications)})
            return
        raise AssertionError((name, actual, expected))

      try:
        match('initial-neutral', bits([0, 0]))
        if mode == 'gamepad-error':
          assert (params / 'JoystickDebugMode').is_dir()
        else:
          assert (params / 'JoystickDebugMode').read_bytes() == b'1'
        if mode == 'keyboard':
          changed = termios.tcgetattr(slave)
          assert changed[3] == normal[3] & ~(termios.ICANON | termios.ECHO)
          keyboard_value = ['']
          owner = original.Keyboard()
          for index, text in enumerate(['WA', 'c', 'q', 'r', 'S' * 25 + 'D' * 25, 'R', 'İ', 'Wa']):
            for character in text:
              keyboard_value[0] = character
              owner.update()
            os.write(master, text.encode())
            match('keys-' + str(index), bits([owner.axes_values[name] for name in owner.axes_order]))
        elif mode in ['gamepad', 'managed-gamepad']:
          writer = os.open(path, os.O_WRONLY | os.O_NONBLOCK)
          original.HARDWARE = types.SimpleNamespace(get_device_type=lambda: 'pc')
          owner = original.Joystick()
          events = [('ABS_Z', 3, 2, 0), ('ABS_RX', 3, 3, 255), ('ABS_RZ', 3, 5, 255),
                    ('ABS_Z', 3, 2, -32768), ('BTN_NORTH', 1, 307, 1), ('SYN_REPORT', 0, 0, 0),
                    ('ABS_RY', 3, 4, 128), ('ABS_Z', 3, 2, 127)]
          if mode == 'managed-gamepad':
            events = events[:1]
          for index, (name, kind, code, value) in enumerate(events):
            previous = bits([owner.axes_values[name] for name in owner.axes_order])
            original.get_gamepad = lambda name=name, value=value: [types.SimpleNamespace(code=name, state=value)]
            owner.update()
            event = struct.pack('=qqHHi', 0, 0, kind, code, value)
            if index == 0 and mode == 'gamepad':
              os.write(writer, event[:8])
              match('partial-event-keeps-neutral', previous)
              os.write(writer, event[8:])
            else:
              os.write(writer, event)
            match('event-' + str(index), bits([owner.axes_values[name] for name in owner.axes_order]))
          os.close(writer)
          writer = None
          if mode == 'gamepad':
            match('end-of-stream-retains-axes', bits([owner.axes_values[name] for name in owner.axes_order]))
        else:
          match('input-oserror-neutral', bits([0, 0]))
        for _ in range(10):
          receive()
        child.send_signal(signal.SIGINT if implementation == 'source' else signal.SIGTERM)
        assert child.wait(timeout=3) == 0
        if mode == 'keyboard':
          assert termios.tcgetattr(slave) == normal
        times = [message['logMonoTime'] for message in publications]
        rate = (len(times) - 1) * 1e9 / (times[-1] - times[0])
        assert 80 <= rate <= 120, rate
        results.append({'implementation': implementation, 'mode': mode, 'phases': len(phases),
          'publications': len(publications), 'rate_hz': rate, 'exit_code': 0,
          'terminal_restored': mode == 'keyboard', 'command': command})
      finally:
        if child.poll() is None:
          child.send_signal(signal.SIGINT if implementation == 'source' else signal.SIGTERM)
          child.wait(timeout=3)
        stdout.close()
        stderr.close()
        if master is not None:
          os.close(master)
        if writer is not None:
          os.close(writer)
        os.close(slave)
        (directory / 'publications.json').write_text(json.dumps(publications) + '\n')
        (directory / 'phases.json').write_text(json.dumps(phases, indent=2) + '\n')

  params = Path(os.environ['PARAMS_ROOT']) / os.environ['OPENPILOT_PREFIX']
  (params / 'IsOffroad').write_text('0')
  if (params / 'JoystickDebugMode').is_dir():
    (params / 'JoystickDebugMode').rmdir()
  (params / 'JoystickDebugMode').write_text('0')
  for implementation in ['source', 'rust']:
    command = ([sys.executable, '-P', str(wrapper), '--binding', str(args.binding.resolve())]
               if implementation == 'source' else [binary]) + ['--keyboard']
    output = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True, check=False)
    assert output.returncode == 0
    assert (params / 'JoystickDebugMode').read_bytes() == b'0'
    (args.output / (implementation + '-offroad-gate.stdout.log')).write_bytes(output.stdout)
  (params / 'IsOffroad').write_text('1')
  invalid = subprocess.run([binary, '--keyboard'], stdin=subprocess.DEVNULL, capture_output=True, check=False)
  assert invalid.returncode == 1
  assert (params / 'JoystickDebugMode').read_bytes() == b'0'
  (args.output / 'non-tty.stderr.log').write_bytes(invalid.stderr)
  receipt = {'runs': results, 'offroad_gate': True, 'zmq_presence_bypass': True, 'non_tty_exit_code': 1,
    'buttons_always_empty': True, 'params_write_io_ignored': True, 'managed_onroad_publishes': True,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()

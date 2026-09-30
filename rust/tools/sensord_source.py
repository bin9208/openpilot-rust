"""Actual LSM drivers and loop bodies with deterministic bus/clock/publisher boundaries."""

import ast
import ctypes
import json
import os
from pathlib import Path
import select
import struct
import sys
import threading
import types
from typing import Optional

import capnp

from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from openpilot.common.gpio import gpioevent_data
from openpilot.common.utils import MovingAverage
from openpilot.system.sensord.sensors import i2c_sensor, lsm6ds3_accel, lsm6ds3_gyro, lsm6ds3_temp

ROOT = Path(__file__).resolve().parents[2]


def definitions(path, names, namespace):
  tree = ast.parse(path.read_text())
  nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names]
  assert len(nodes) == len(names)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)


def main():
  request = json.load(sys.stdin)
  registers = [0] * 256
  registers[15], registers[13] = request.get('chip', 0x6A), 0x80
  state = {'mono': 1.0, 'offset': 1000000000, 'ready': 3, 'fault': None, 'index': 0}
  trace, packets, logs, rows = [], [], [], []
  frames = request.get('frames', [])

  class Clock:
    @staticmethod
    def monotonic():
      return state['mono']

    @staticmethod
    def monotonic_ns():
      return int(state['mono'] * 1e9)

    @staticmethod
    def time_ns():
      return Clock.monotonic_ns() + state['offset']

    @staticmethod
    def sleep(seconds):
      trace.append(['sleep', seconds])

  class Bus:
    count = 0

    def __init__(self, bus):
      assert bus == 1
      self.index = Bus.count
      Bus.count += 1

    def close(self):
      pass

    def read_i2c_block_data(self, address, reg, length):
      assert address == 0x6A
      trace.append([self.index, 'read', reg, length])
      if state['fault'] == reg:
        state['fault'] = None
        raise OSError(5, 'Input/output error')
      if reg == 0x1E:
        return [state['ready']]
      if reg in (0x28, 0x22):
        if reg == 0x28:
          values = [2100, 1800, 18384] if registers[20] & 3 and not request.get('fail_self_test') else [100, -200, 16384]
        else:
          values = [4900, 5100, 5200] if registers[20] & 12 and not request.get('fail_self_test') else [-100, 100, 200]
        return list(struct.pack('<3h', *values)[:length])
      if reg == 0x20:
        return list(struct.pack('<h', -256))
      return [registers[(reg + i) % 256] for i in range(length)]

    def write_byte_data(self, address, reg, value):
      assert address == 0x6A
      trace.append([self.index, 'write', reg, value])
      registers[reg] = value

  i2c_sensor.SMBus = Bus
  for module in (i2c_sensor, lsm6ds3_accel, lsm6ds3_gyro, lsm6ds3_temp):
    module.time = Clock
  if request.get('self_test') is None:
    os.environ.pop('LSM_SELF_TEST', None)
  else:
    os.environ['LSM_SELF_TEST'] = request['self_test']
  sensors = [lsm6ds3_accel.LSM6DS3_Accel(1), lsm6ds3_gyro.LSM6DS3_Gyro(1), lsm6ds3_temp.LSM6DS3_Temp(1)]

  def call(operation):
    try:
      value = operation()
      rows.append({'result': value.to_dict() if hasattr(value, 'to_dict') else value})
    except (i2c_sensor.Sensor.DataNotReady, OSError, AssertionError) as error:
      rows.append({'error': type(error).__name__})
    except Exception as error:
      rows.append({'error': str(error)})

  for sensor in sensors:
    call(sensor.reset)
  for sensor in sensors:
    call(sensor.init)

  class Publisher:
    def __init__(self, services):
      self.services = services

    def send(self, service, message):
      assert service in self.services
      packets.append({'service': service, 'valid': message.valid, 'logMonoTime': message.logMonoTime, 'event': getattr(message, service).to_dict()})

  class Logger:
    @staticmethod
    def error(text):
      logs.append(['error', text])

    @staticmethod
    def warning(text):
      logs.append(['warning', text])

    @staticmethod
    def exception(text):
      logs.append(['exception', text])

  messaging = {'log': log, 'time': Clock, 'Optional': Optional, 'capnp': capnp}
  definitions(ROOT / 'openpilot/cereal/messaging/__init__.py', {'new_message'}, messaging)
  message_boundary = types.SimpleNamespace(PubMaster=Publisher, new_message=messaging['new_message'])
  rate = {'time': Clock, 'MovingAverage': MovingAverage, 'getproctitle': lambda: 'fixture'}
  definitions(ROOT / 'openpilot/common/realtime.py', {'Ratekeeper'}, rate)

  def next_frame():
    frame = frames[state['index']]
    state['index'] += 1
    state.update(mono=frame.get('mono', 1.0), offset=frame.get('offset', 1000000000), ready=frame.get('ready', 3), fault=frame.get('fault'))
    state['frame'] = frame
    return frame

  class Poller:
    def register(self, fd, flags):
      assert fd == 7 and flags == select.POLLIN | select.POLLPRI

    def poll(self, timeout):
      assert timeout == 100
      frame = next_frame()
      if frame.get('poll') == 'timeout':
        return []
      return [(7, select.POLLHUP if frame.get('poll') == 'other' else select.POLLIN)]

  def read(fd, size):
    assert fd == 7 and size == 256
    frame = state['frame']
    return b'\0' * 4 if frame.get('poll') == 'short' else struct.pack('=Q', frame.get('timestamp', 3000000000)) + b'\0' * 24

  class Stop:
    def is_set(self):
      if state['index'] >= len(frames):
        return True
      if request.get('mode') == 'poll':
        next_frame()
      return False

  loop = {
    'Sensor': i2c_sensor.Sensor,
    'threading': threading,
    'ctypes': ctypes,
    'gpioevent_data': gpioevent_data,
    'time': Clock,
    'messaging': message_boundary,
    'cloudlog': Logger,
    'SERVICE_LIST': SERVICE_LIST,
    'Ratekeeper': rate['Ratekeeper'],
    'gpiochip_get_ro_value_fd': lambda *args: 7,
    'select': types.SimpleNamespace(poll=Poller, POLLIN=select.POLLIN, POLLPRI=select.POLLPRI),
    'os': types.SimpleNamespace(path=types.SimpleNamespace(exists=lambda path: False), read=read),
  }
  definitions(ROOT / 'openpilot/system/sensord/sensord.py', {'interrupt_loop', 'polling_loop'}, loop)
  mode = request.get('mode', 'drivers')
  if mode == 'drivers':
    for sensor in sensors:
      call(lambda sensor=sensor: sensor.get_event(1234567890))
    for mono in (1.0, 1.5, 1.500001):
      state['mono'] = mono
      rows.append({'result': [sensor.is_data_valid() for sensor in sensors]})
  elif mode == 'irq':
    try:
      loop['interrupt_loop']([(sensors[0], 'accelerometer', True), (sensors[1], 'gyroscope', True), (sensors[2], 'temperatureSensor', False)], Stop())
    except ValueError as error:
      assert 'Buffer size too small' in str(error)
      rows.append({'error': 'short GPIO event'})
  elif mode == 'poll':
    loop['polling_loop'](sensors[2], 'temperatureSensor', Stop())
  else:
    raise ValueError(mode)
  for sensor in sensors:
    call(sensor.shutdown)
  json.dump({'rows': rows, 'trace': trace, 'logs': logs, 'packets': packets, 'registers': registers}, sys.stdout)


if __name__ == '__main__':
  main()

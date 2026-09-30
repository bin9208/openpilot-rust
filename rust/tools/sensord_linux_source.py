"""Actual common/i2c.py and gpio.py ioctl calls against the selected regular files."""

import ast
import gc
import json
import os
from pathlib import Path
import select
import sys
import types

from openpilot.common import gpio, i2c


def main():
  device, chip = sys.argv[1:]
  original_open = os.open
  i2c.os = types.SimpleNamespace(open=lambda path, flags: original_open(device, flags), close=os.close, O_RDWR=os.O_RDWR)
  gpio.os = types.SimpleNamespace(open=lambda path, flags: original_open(chip, flags), close=os.close, O_RDONLY=os.O_RDONLY)
  source = Path(__file__).resolve().parents[2] / 'openpilot/common/realtime.py'
  nodes = [
    node for node in ast.parse(source.read_text()).body if isinstance(node, ast.FunctionDef) and node.name in ('set_core_affinity', 'config_realtime_process')
  ]
  namespace = {'gc': gc, 'sys': sys, 'PC': False, 'os': os}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(source), 'exec'), namespace)
  namespace['config_realtime_process']([1], 1)
  bus = i2c.SMBus(1)
  rows = [bus.read_byte_data(0x6A, 0x0F)]
  bus.write_byte_data(0x6A, 0x60, -1, force=True)
  rows.append(bus.read_byte_data(0x6A, 0x60))
  for reg, length in [(0x40, 6), (0x41, 6), (0x42, 6), (0x43, 32), (0x44, 0), (0x44, 33), (0xEE, 6)]:
    try:
      rows.append({'bytes': bus.read_i2c_block_data(0x6A, reg, length)})
    except (OSError, ValueError):
      rows.append({'error': True})
  fd = gpio.gpiochip_get_ro_value_fd('sensord', 0, 84)
  poll = select.poll()
  poll.register(fd, select.POLLIN | select.POLLPRI)
  assert poll.poll(100)
  rows.append({'gpio': list(os.read(fd, 256))})
  os.close(fd)
  bus.close()
  json.dump(rows, sys.stdout)


if __name__ == '__main__':
  main()

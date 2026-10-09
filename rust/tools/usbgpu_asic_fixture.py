"""Owned AMD hardware boundary for original/native boot sequence comparison."""

from __future__ import annotations
import argparse
import ast
import ctypes
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))
os.environ['DEV'] = 'AMD::gfx1200'
from tinygrad.runtime.support.am import amdev, ip
from tinygrad.runtime.autogen.am import smu_14_0_2
from test.mockgpu.am.amgpu import MockAMGPU, MockSMU


class Boundary:
  def __init__(self, initial, fault):
    self.gpu = MockAMGPU()
    self.fault = fault
    self.trace = []
    self.clock = 0
    self.sram = bytearray(512 << 10)
    self.config = bytearray(4096)
    self.sram_used = 0
    self.smu = next(block for block in self.gpu.mmio.blocks if isinstance(block, MockSMU))
    self.original_smu_read = self.smu.read
    self.smu.read = self.smu_read
    if initial != 'cold':
      scratch = next(block._regs for block in self.gpu.mmio.blocks if 'regSCRATCH_REG7' in block._regs)
      self.gpu.mmio.regs[scratch['regSCRATCH_REG7'].addr[0]] = amdev.AMDev.Version
      self.gpu.mmio.regs[scratch['regSCRATCH_REG6'].addr[0]] = int(initial == 'dirty')
      for block in self.gpu.mmio.blocks:
        if hasattr(block, '_sos_alive'):
          block._sos_alive = True

  def smu_read(self, register):
    if register == self.smu._c2pmsg_82:
      message = self.gpu.mmio.regs.get(self.smu._c2pmsg_66, 0)
      parameter = self.gpu.mmio.regs.get(register, 0)
      if message == smu_14_0_2.PPSMC_MSG_GetDpmFreqByIndex:
        return 3 if parameter & 255 == 255 else [400, 800, 1200][parameter & 255]
    return self.original_smu_read(register)

  def call(self, request):
    self.clock += 1
    op = request['op']
    record = request.copy()
    match op:
      case 'reg_read':
        value = self.gpu.mmio[request['index']]
      case 'reg_write':
        self.gpu.mmio[request['index']] = request['value']
        if self.fault == 'psp_error':
          for block in self.gpu.mmio.blocks:
            if hasattr(block, '_c2pmsg_67') and request['index'] == block._c2pmsg_67:
              ring = self.gpu.mmio.regs[block._c2pmsg_69] | (self.gpu.mmio.regs.get(block._c2pmsg_70, 0) << 32)
              previous = request['value'] - ctypes.sizeof(amdev.am.struct_psp_gfx_rb_frame) // 4
              frame = amdev.am.struct_psp_gfx_rb_frame.from_buffer_copy(
                bytes(self.gpu.vram[ring + previous * 4 : ring + previous * 4 + ctypes.sizeof(amdev.am.struct_psp_gfx_rb_frame)])
              )
              command = frame.cmd_buf_addr_lo | (frame.cmd_buf_addr_hi << 32)
              response = amdev.am.struct_psp_gfx_cmd_resp.from_buffer(self.gpu.vram, command)
              response.resp.status = 5
        value = None
      case 'vram_read' | 'sram_read':
        data = self.gpu.vram if op == 'vram_read' else self.sram
        offset = request['address'] if op == 'vram_read' else request['address'] - 0xF000
        value = list(data[offset : offset + request['size']])
      case 'vram_write' | 'sram_write':
        data = self.gpu.vram if op == 'vram_write' else self.sram
        offset = request['address'] if op == 'vram_write' else request['address'] - 0xF000
        data[offset : offset + len(request['data'])] = request['data']
        record['size'] = len(request['data'])
        record['sha256'] = hashlib.sha256(bytes(record.pop('data'))).hexdigest()
        value = None
      case 'value_read':
        value = int.from_bytes(bytes(self.gpu.vram[request['address'] : request['address'] + request['size']]), 'little')
      case 'value_write':
        data = request['value'].to_bytes(request['size'], 'little')
        self.gpu.vram[request['address'] : request['address'] + request['size']] = list(data)
        value = None
      case 'config_read':
        value = int.from_bytes(self.config[request['offset'] : request['offset'] + request['size']], 'little')
      case 'config_write':
        self.config[request['offset'] : request['offset'] + request['size']] = request['value'].to_bytes(request['size'], 'little')
        value = None
      case 'alloc_sram':
        offset = self.sram_used
        self.sram_used += request['size']
        assert self.sram_used <= len(self.sram)
        value = [0xF000 + offset, 0x200000 + offset]
      case 'sleep':
        self.clock += request['milliseconds']
        value = None
      case _:
        raise ValueError(request)
    record['result'] = {'size': len(value), 'sha256': hashlib.sha256(bytes(value)).hexdigest()} if isinstance(value, list) and op.endswith('_read') else value
    self.trace.append(record)
    return value


class View:
  def __init__(self, boundary, address, size, fmt='B', sram=False):
    self.boundary, self.address, self.nbytes, self.fmt, self.sram = boundary, address, size, fmt, sram
    self.element = struct.calcsize(fmt)

  def __len__(self):
    return self.nbytes // self.element

  def view(self, offset=0, size=None, fmt=None):
    return View(self.boundary, self.address + offset, size or self.nbytes - offset, fmt or self.fmt, self.sram)

  def __getitem__(self, index):
    if isinstance(index, slice):
      start, stop = index.start or 0, index.stop if index.stop is not None else len(self)
      return bytes(
        self.boundary.call(
          {'op': 'sram_read' if self.sram else 'vram_read', 'address': self.address + start * self.element, 'size': (stop - start) * self.element}
        )
      )
    return self.boundary.call({'op': 'value_read', 'address': self.address + index * self.element, 'size': self.element})

  def __setitem__(self, index, value):
    if isinstance(index, slice):
      start = index.start or 0
      self.boundary.call({'op': 'sram_write' if self.sram else 'vram_write', 'address': self.address + start * self.element, 'data': list(bytes(value))})
    else:
      self.boundary.call({'op': 'value_write', 'address': self.address + index * self.element, 'size': self.element, 'value': value})


class Mmio:
  def __init__(self, boundary):
    self.boundary = boundary

  def __len__(self):
    return 0x10000000

  def __getitem__(self, index):
    return self.boundary.call({'op': 'reg_read', 'index': index})

  def __setitem__(self, index, value):
    self.boundary.call({'op': 'reg_write', 'index': index, 'value': value})


class Pci:
  pcibus = 'usb:fixture'

  def __init__(self, boundary):
    self.boundary = boundary

  def map_bar(self, index, fmt='B'):
    return Mmio(self.boundary) if index == 5 else View(self.boundary, 0, (512 << 20) if index == 0 else 0x2000, fmt)

  def alloc_sysmem(self, size):
    address, physical = self.boundary.call({'op': 'alloc_sram', 'size': size})
    return View(self.boundary, address, size, sram=True), [physical]

  def read_config(self, offset, size):
    return self.boundary.call({'op': 'config_read', 'offset': offset, 'size': size})

  def write_config_flush(self, offset, value, size):
    self.boundary.call({'op': 'config_write', 'offset': offset, 'size': size, 'value': value})
    self.read_config(offset, size)


def prepare_source(boundary, firmware, power):
  def fetch(path, name, expected):
    assert path == 'amdgpu'
    data = (firmware / name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == expected
    return data

  amdev.fetch_fw = fetch
  fake_time = types.SimpleNamespace(
    perf_counter=lambda: boundary.clock / 1000, sleep=lambda delay: boundary.call({'op': 'sleep', 'milliseconds': round(delay * 1000)})
  )
  path = ROOT / 'tinygrad_repo/tinygrad/helpers.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'wait_cond']
  env = {'time': fake_time}
  exec(compile(tree, str(path), 'exec'), env)
  ip.wait_cond, ip.time = env['wait_cond'], fake_time
  if power is not None:
    os.environ['AM_POWER_LIMIT'] = str(power)
  return fake_time


def original(boundary, firmware, power, queues):
  prepare_source(boundary, firmware, power)
  device = amdev.AMDev(Pci(boundary))
  if queues:
    ring = device.mm.valloc(8192, uncached=True, contiguous=True)
    gart = device.mm.valloc(4096, uncached=True, contiguous=True)
    eop = device.mm.valloc(4096)
    device.gfx.setup_ring(ring.va_addr, ring.size, gart.va_addr + 128, gart.va_addr + 56, eop.va_addr, eop.size, 0, False)
    ring = device.mm.valloc(4096, uncached=True, contiguous=True)
    gart = device.mm.valloc(4096, uncached=True, contiguous=True)
    device.sdma.setup_ring(ring.va_addr, ring.size, gart.va_addr + 128, gart.va_addr + 56, 0)
  result = {'partial_boot': device.partial_boot, 'error_state': device.is_err_state, 'vram_size': device.vram_size}
  device.fini()
  result["error_state"] = device.is_err_state
  return result


def native(boundary, firmware, binary, power, queues):
  command = [binary, str(firmware)] + ([str(power)] if power is not None else [])
  if queues:
    command.append('--queues')
  process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  done = None
  for line in process.stdout:
    request = json.loads(line)
    if 'done' in request:
      done = request['done']
      break
    try:
      response = {'value': boundary.call(request)}
    except Exception as error:
      response = {'error': str(error)}
    process.stdin.write(json.dumps(response) + '\n')
    process.stdin.flush()
  process.stdin.close()
  stderr = process.stderr.read()
  code = process.wait(timeout=10)
  return {'result': done, 'exit_code': code, 'stderr': stderr, 'invocation': command}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--kind', choices=['source', 'native'], required=True)
  parser.add_argument('--initial', choices=['cold', 'warm', 'dirty'], default='cold')
  parser.add_argument('--fault', choices=['none', 'psp_error'], default='none')
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--binary')
  parser.add_argument('--power', type=float)
  parser.add_argument('--queues', action='store_true')
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  boundary = Boundary(args.initial, args.fault)
  try:
    if args.kind == 'source':
      result = original(boundary, args.firmware, args.power, args.queues)
    else:
      result = native(boundary, args.firmware, args.binary, args.power, args.queues)
  except Exception as error:
    result = {'error': str(error)}
  args.output.write_text(json.dumps({'result': result, 'trace': boundary.trace}, indent=2) + '\n')
  print(args.kind, args.initial, len(boundary.trace), 'events')


if __name__ == '__main__':
  main()

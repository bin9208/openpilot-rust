"""Full source AMDDevice versus Rust runtime at an owned MMIO/USB memory boundary."""

from __future__ import annotations
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import types
from usbgpu_asic_fixture import Boundary as BootBoundary, Pci as BootPci, View, Mmio, prepare_source


class Boundary(BootBoundary):
  def __init__(self):
    super().__init__('cold', 'none')
    self.controller = bytearray(0x10000)
    self.sram_host = (ctypes.c_ubyte * len(self.sram)).from_buffer(self.sram)
    self.controller_host = (ctypes.c_ubyte * len(self.controller)).from_buffer(self.controller)
    for offset in range(0, len(self.sram), 4096):
      self.gpu._sysmem_map[0x200000 + offset] = ctypes.addressof(self.sram_host) + offset
    for physical, address in [(0x820000, 0xA000), (0x822000, 0xB800)]:
      self.gpu._sysmem_map[physical] = ctypes.addressof(self.controller_host) + address

  def call(self, request):
    op = request['op']
    if op not in {'cache_doorbells', 'cache_vram', 'controller_read', 'controller_write', 'doorbell_write', 'arm_read', 'barrier'}:
      return super().call(request)
    self.clock += 1
    record, value = request.copy(), None
    if op.startswith('controller_'):
      address = request['address']
      data, offset = (self.sram, address - 0xF000) if address >= 0xF000 else (self.controller, address)
      if op == 'controller_read':
        value = list(data[offset : offset + request['size']])
      else:
        raw = bytes(request['data'])
        data[offset : offset + len(raw)] = raw
        record['size'], record['sha256'] = len(raw), hashlib.sha256(raw).hexdigest()
        del record['data']
    record['result'] = {'size': len(value), 'sha256': hashlib.sha256(bytes(value)).hexdigest()} if isinstance(value, list) else value
    self.trace.append(record)
    return value


class RuntimeView(View):
  def __init__(self, boundary, address, size, fmt='B', controller=False, doorbell=False):
    super().__init__(boundary, address, size, fmt)
    self.controller, self.doorbell, self.addr = controller, doorbell, address

  def view(self, offset=0, size=None, fmt=None):
    return RuntimeView(self.boundary, self.address + offset, size or self.nbytes - offset, fmt or self.fmt, self.controller, self.doorbell)

  def __getitem__(self, index):
    if not self.controller:
      return super().__getitem__(index)
    start = (index.start or 0) if isinstance(index, slice) else index
    size = ((index.stop if index.stop is not None else len(self)) - start) * self.element if isinstance(index, slice) else self.element
    value = bytes(self.boundary.call({'op': 'controller_read', 'address': self.address + start * self.element, 'size': size}))
    return value if isinstance(index, slice) else int.from_bytes(value, 'little')

  def __setitem__(self, index, value):
    if self.doorbell:
      self.boundary.call({'op': 'doorbell_write', 'index': self.address // 8 + index, 'value': value})
    elif self.controller:
      start = (index.start or 0) if isinstance(index, slice) else index
      data = bytes(value) if isinstance(index, slice) else value.to_bytes(self.element, 'little')
      self.boundary.call({'op': 'controller_write', 'address': self.address + start * self.element, 'data': list(data)})
    else:
      super().__setitem__(index, value)


class CacheList(list):
  def __init__(self, boundary):
    super().__init__()
    self.boundary = boundary

  def __iadd__(self, ranges):
    for address, size in ranges:
      self.boundary.call({'op': 'cache_doorbells'} if address == 0xCAFE0000 else {'op': 'cache_vram', 'offset': address, 'size': size})
    return super().__iadd__(ranges)


class Pci(BootPci):
  def __init__(self, boundary, custom):
    super().__init__(boundary)
    self.usb = types.SimpleNamespace(
      usb=types.SimpleNamespace(is_custom=custom),
      _pci_cacheable=CacheList(boundary),
      scsi_read_arm=lambda size: boundary.call({'op': 'arm_read', 'size': size}),
    )

  def bar_info(self, index):
    return (0xCAFE0000, 0x2000) if index == 2 else (0, 512 << 20)

  def map_bar(self, index=None, fmt='B', off=0, size=None, bar=None):
    index = bar if bar is not None else index
    if index == 5:
      return Mmio(self.boundary)
    return RuntimeView(self.boundary, off, size or ((512 << 20) if index == 0 else 0x2000), fmt, doorbell=index == 2)

  def dma_view(self, ctrl_addr, size):
    return RuntimeView(self.boundary, ctrl_addr, size, controller=True)


def original(boundary, firmware, custom, aql, no_copy):
  from tinygrad.runtime import ops_amd
  from tinygrad.runtime.support import hcq

  clock = prepare_source(boundary, firmware, None)
  ops_amd.time = hcq.time = clock
  ops_amd.USB3 = types.SimpleNamespace(list_devices=lambda vendor, product: [(object(), 'fixture')] if vendor == 0xADD1 else [])
  ops_amd.USBPCIDevice = lambda *args: Pci(boundary, custom)
  ops_amd.System.memory_barrier = lambda: boundary.call({'op': 'barrier'})
  ops_amd.AMDDevice.ifaces = [ops_amd.USBIface]
  os.environ['AMD_AQL'], os.environ['AMD_DISABLE_SDMA'] = str(int(aql)), str(int(no_copy))
  device = ops_amd.AMDDevice('AMD')
  props = device.iface.props
  result = {
    'properties': {
      'target': list(device.target),
      'gc_version': list(device.iface.ip_versions[ops_amd.am.GC_HWIP]),
      'nbio_version': list(device.iface.ip_versions[ops_amd.am.NBIF_HWIP]),
      'sdma_version': list(device.iface.ip_versions[ops_amd.am.SDMA0_HWIP]),
      'xccs': device.xccs,
      'cu_count': device.cu_cnt,
      'shader_engines': device.se_cnt,
      'slots_per_cu': props['max_slots_scratch_cu'],
      'waves_per_cu': device.waves_per_cu,
      'lds_kib': props['lds_size_in_kb'],
    },
    'system_next': device.iface.sys_next_off,
    'staging': device.iface.copy_bufs[0].va_addr,
    'completion': device.iface.cq_buf.va_addr,
    'timeline': device.timeline_signal.value_addr,
    'next_timeline': device.timeline_value,
  }
  device.iface.dev_impl.fini()
  return result


def native(boundary, firmware, binary, custom, aql, no_copy):
  command = [binary, str(firmware)] + ([] if custom else ['--stock']) + (['--aql'] if aql else []) + (['--no-copy'] if no_copy else [])
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
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--binary')
  parser.add_argument('--stock', action='store_true')
  parser.add_argument('--aql', action='store_true')
  parser.add_argument('--no-copy', action='store_true')
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  boundary = Boundary()
  try:
    result = (
      original(boundary, args.firmware, not args.stock, args.aql, args.no_copy)
      if args.kind == 'source'
      else native(boundary, args.firmware, args.binary, not args.stock, args.aql, args.no_copy)
    )
  except Exception as error:
    import traceback

    traceback.print_exc()
    result = {'error': str(error)}
  args.output.write_text(json.dumps({'result': result, 'trace': boundary.trace}, indent=2) + '\n')
  print(args.kind, len(boundary.trace), 'events')


if __name__ == '__main__':
  main()

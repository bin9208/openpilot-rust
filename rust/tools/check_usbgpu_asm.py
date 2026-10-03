"""Compare unchanged ASM methods to native Rust using only USB/clock fixtures."""

from __future__ import annotations
import argparse
import ast
import contextlib
import ctypes
import functools
import json
import pathlib
import struct
import subprocess
import types

ROOT = pathlib.Path(__file__).resolve().parents[2]


def original(case):
  source = ROOT / 'tinygrad_repo/tinygrad/runtime/support/usb.py'
  tree = ast.parse(source.read_text())
  tree.body = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)] + [
    n for n in tree.body if isinstance(n, (ast.FunctionDef, ast.ClassDef)) and n.name in {'checked', 'USB3', 'CustomASM24Controller'}
  ]
  ast.fix_missing_locations(tree)
  replies = iter(case['replies'])
  trace = []
  waits = []
  consumed = 0

  def response(data, length):
    nonlocal consumed
    reply = next(replies)
    consumed += 1
    for i, value in enumerate(reply.get('data', [])):
      if i < length:
        data[i] = value
    return reply

  def control(handle, kind, request, value, index, data, length, timeout):
    trace.append(
      {
        'kind': kind,
        'request': request,
        'value': value,
        'index': index,
        'timeout': timeout,
        'length': length,
        'data': list(data[:length]) if data is not None and kind & 0x80 == 0 else ([] if kind & 0x80 == 0 else None),
      }
    )
    return response(data, length).get('code', length)

  def bulk(handle, endpoint, data, length, actual, timeout):
    trace.append({'bulk': endpoint, 'length': length, 'timeout': timeout, 'data': list(data[:length]) if endpoint & 0x80 == 0 else None})
    reply = response(data, length)
    actual.value = reply.get('actual', length)
    return reply.get('code', 0)

  def alloc(length):
    data = (ctypes.c_ubyte * length)()
    return data, memoryview(data).cast('B')

  env = {
    'ctypes': ctypes,
    'struct': struct,
    'functools': functools,
    'contextlib': contextlib,
    'DEBUG': 0,
    'alloc_cbuffer': alloc,
    'round_up': lambda n, d: (n + d - 1) // d * d,
    'ceildiv': lambda n, d: (n + d - 1) // d,
    'usbgpu_bus_lock': contextlib.nullcontext,
    'time': types.SimpleNamespace(sleep=lambda n: waits.append(round(n * 1000)), monotonic=lambda: sum(waits) / 1000),
    'libusb': types.SimpleNamespace(libusb_control_transfer=control, libusb_bulk_transfer=bulk, LIBUSB_ERROR_IO=-1, libusb_strerror=lambda code: b'fixture'),
  }
  exec(compile(tree, str(source), 'exec'), env)
  usb = env['USB3'].__new__(env['USB3'])
  usb.handle = 1
  usb._transferred = ctypes.c_int(0)
  usb._bulk_out_buf, usb._bulk_out_mv = alloc(65536)
  usb._bulk_in_buf, usb._bulk_in_mv = alloc(65536)
  controller = env['CustomASM24Controller'](usb)
  values = []
  error = None
  for operation in case['operations']:
    kind = operation['kind']
    address = operation.get('address', 0)
    length = operation.get('length', 0)
    try:
      if kind == 'read':
        v = controller.read(address, length)
      elif kind == 'write':
        v = controller.write(address, bytes(operation['data']))
      elif kind == 'power':
        v = controller.set_pcie_power(operation['on'])
      elif kind == 'request':
        v = controller.pcie_request(operation['format'], address, operation.get('value'), operation['size'])
      elif kind == 'cache':
        controller._pci_cacheable.append((address, length))
        v = None
      elif kind == 'memory_read':
        v = controller.pcie_mem_read(address, length)
      elif kind == 'memory_write':
        v = controller.pcie_mem_write(address, operation['data'], 4)
      elif kind == 'scsi_write':
        v = controller.scsi_write(bytes(operation['data']))
      elif kind == 'scsi_read_arm':
        v = controller.scsi_read_arm(length)
      elif kind == 'scsi_read':
        v = controller.scsi_read(length)
      else:
        raise AssertionError(kind)
      values.append(list(v) if isinstance(v, (bytes, memoryview)) else v)
    except (AssertionError, RuntimeError) as exc:
      error = str(exc)
      break
  return {'values': values, 'error': error, 'trace': trace, 'waits': waits, 'remaining': len(case['replies']) - consumed}


def scenarios():
  yield {'name': 'startup-power-recovery', 'replies': [{'data': [0]}, {}, {'data': [0]}, {'data': [0x78]}], 'operations': []}
  yield {
    'name': 'chunked-read',
    'replies': [{'data': [0x78]}, {'data': [i % 256 for i in range(255)]}, {'data': [99] * 45}],
    'operations': [{'kind': 'read', 'address': 0x200, 'length': 300}],
  }
  for count in [1, 3, 19, 20]:
    yield {
      'name': f'read-eio-{count}',
      'replies': [{'data': [0x78]}] + [{'code': -1}] * count + ([{'data': [123]}] if count < 20 else []),
      'operations': [{'kind': 'read', 'address': 0xB450, 'length': 1}],
    }
  for code in [-4, 0]:
    yield {'name': f'read-non-eio-{code}', 'replies': [{'data': [0x78]}, {'code': code}], 'operations': [{'kind': 'read', 'address': 0x100, 'length': 2}]}
  for offset in range(4):
    for size in range(1, 5):
      yield {
        'name': f'tlp-read-{offset}-{size}',
        'replies': [{'data': [0x78]}, {}, {'data': [0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0]}],
        'operations': [{'kind': 'request', 'format': 0x20, 'address': 0x123400 + offset, 'size': size}],
      }
  for status in [1, 2, 3, 4, 7]:
    yield {
      'name': f'tlp-status-{status}',
      'replies': [{'data': [0x78]}, {}, {'data': [0, 0, 0, 0, status << 5, 0, 0, 0]}],
      'operations': [{'kind': 'request', 'format': 0x20, 'address': 0x100, 'size': 4}],
    }
  for retry in [1, 5, 11]:
    replies = [{'data': [0x78]}] + [v for _ in range(retry) for v in [{}, {'data': [0, 0, 0, 0, 0, 0, 0, 1]}]]
    if retry < 11:
      replies += [{}, {'data': [1, 0, 0, 0, 0, 0, 0, 0]}]
    yield {'name': f'tlp-retry-{retry}', 'replies': replies, 'operations': [{'kind': 'request', 'format': 0x20, 'address': 0x100, 'size': 4}]}
  yield {
    'name': 'cached-writes',
    'replies': [{'data': [0x78]}, {}, {}],
    'operations': [
      {'kind': 'cache', 'address': 0x100, 'length': 4},
      {'kind': 'request', 'format': 0x60, 'address': 0x100, 'size': 4, 'value': 3},
      {'kind': 'request', 'format': 0x60, 'address': 0x100, 'size': 4, 'value': 3},
      {'kind': 'request', 'format': 0x60, 'address': 0x104, 'size': 4, 'value': 7},
    ],
  }
  yield {
    'name': 'writes-and-streams',
    'replies': [{'data': [0x78]}] + [{}] * 7 + [{'data': [1, 2, 3, 4]}],
    'operations': [
      {'kind': 'write', 'address': 0x200, 'data': [1, 2, 3]},
      {'kind': 'power', 'on': False},
      {'kind': 'memory_write', 'address': 0x100000000, 'data': [0x12345678, 0xABCDEF01]},
      {'kind': 'memory_read', 'address': 0x100000000, 'length': 4},
    ],
  }
  for length in [0, 1, 511, 512, 513, 16384, 16385]:
    yield {
      'name': f'sram-{length}',
      'replies': [{'data': [0x78]}, {}, {}, {}],
      'operations': [{'kind': 'scsi_write', 'data': [7] * length}, {'kind': 'scsi_read_arm', 'length': length}],
    }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=pathlib.Path, required=True)
  parser.add_argument('--evidence', type=pathlib.Path, required=True)
  args = parser.parse_args()
  cases = list(scenarios())
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  native = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(native) == len(cases)
  results = []
  for case, actual in zip(cases, native, strict=True):
    expected = original(case)
    assert expected['remaining'] == 0, (case['name'], expected['remaining'])
    if actual['error'] is not None:
      actual['error'] = actual['error'].removeprefix('USB protocol: ')
    results.append({'name': case['name'], 'input': case, 'source': expected, 'native': actual, 'passed': expected == actual})
  args.evidence.parent.mkdir(parents=True, exist_ok=True)
  args.evidence.write_text(json.dumps({'invocation': [str(args.binary)], 'results': results}, indent=2) + '\n')
  failures = [result['name'] for result in results if not result['passed']]
  assert not failures, failures
  print(f'PASS {len(results)} original-source ASM command traces')


if __name__ == '__main__':
  main()

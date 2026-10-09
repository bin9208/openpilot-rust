from __future__ import annotations
import ctypes
import hashlib
import json
import math
import struct


def virtual_bytes(gpu, address, size):
  result = bytearray()
  while size:
    count = min(size, 4096 - address % 4096)
    result.extend(ctypes.string_at(gpu.translate_addr(address), count))
    address, size = address + count, size - count
  return bytes(result)


def zero_virtual(gpu, address, size):
  while size:
    count = min(size, 4096 - address % 4096)
    ctypes.memset(gpu.translate_addr(address), 0, count)
    address, size = address + count, size - count


def compare(boundary, snapshot, library_path, evidence):
  outputs = [binding for binding in snapshot['bindings'] if binding['output']]
  before = {binding['name']: virtual_bytes(boundary.gpu, binding['address'], binding['bytes']) for binding in outputs}
  for binding in outputs:
    if binding['alias'] is not None:
      zero_virtual(boundary.gpu, binding['address'], binding['bytes'])
  start_kernel, start_event = boundary.kernels, len(boundary.trace)

  def control(handle, kind, request, value, index, pointer, size, timeout):
    return boundary.call(
      {
        'op': 'usb_control',
        'type': kind,
        'request': request,
        'value': value,
        'index': index,
        'timeout': timeout,
        'data': list(ctypes.string_at(pointer, size)),
      }
    )['code']

  def bulk(handle, endpoint, pointer, size, transferred, timeout):
    result = boundary.call(
      {
        'op': 'usb_bulk',
        'endpoint': endpoint,
        'timeout': timeout,
        'data': list(bytes(size) if endpoint & 128 else ctypes.string_at(pointer, size)),
      }
    )
    if endpoint & 128:
      ctypes.memmove(pointer, bytes(result['data']), size)
    if transferred:
      ctypes.c_int.from_address(transferred).value = result['actual']
    return result['code']

  callbacks = [
    ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_uint64, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int, ctypes.c_int)(
      control
    ),
    ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_uint64, ctypes.c_int, ctypes.c_void_p, ctypes.c_int, ctypes.c_uint64, ctypes.c_int)(bulk),
  ]
  buffers = []
  for parameter in snapshot['parameters']:
    data = bytearray(parameter['data'])
    if parameter['slot'] in (0, 3):
      data[:8] = ctypes.cast(callbacks[parameter['slot'] // 3], ctypes.c_void_p).value.to_bytes(8, 'little')
    buffers.append((ctypes.c_ubyte * len(data)).from_buffer_copy(data))
  library = ctypes.CDLL(str(library_path.resolve()))
  library.run.argtypes, library.run.restype = [ctypes.POINTER(ctypes.c_void_p)], None
  library.run((ctypes.c_void_p * len(buffers))(*[ctypes.addressof(buffer) for buffer in buffers]))
  rows = []
  for binding in outputs:
    name = binding['name']
    source = virtual_bytes(boundary.gpu, binding['address'], binding['bytes'])
    (evidence / f'source-{name}.bin').write_bytes(source)
    (evidence / f'native-{name}.bin').write_bytes(before[name])
    rows.append(
      {
        'name': name,
        'bytes': len(source),
        'exact': source == before[name],
        'source_sha256': hashlib.sha256(source).hexdigest(),
        'native_sha256': hashlib.sha256(before[name]).hexdigest(),
      }
    )
  native_finite = sum(math.isfinite(value[0]) for value in struct.iter_unpack('<f', before['outputs']))
  source_output = (evidence / 'source-outputs.bin').read_bytes()
  source_finite = sum(math.isfinite(value[0]) for value in struct.iter_unpack('<f', source_output))
  parity = all(row['exact'] for row in rows)
  result = {
    'passed': parity and native_finite * 4 == len(source_output) and source_finite == native_finite,
    'parity': parity,
    'source_finite': source_finite,
    'native_finite': native_finite,
    'outputs': rows,
    'source_kernels': boundary.kernels - start_kernel,
    'source_transfers': len(boundary.trace) - start_event,
    'source_library_sha256': hashlib.sha256(library_path.read_bytes()).hexdigest(),
    'scope': (
      'Original artifact C dispatcher versus Rust VM on identical native-linked buffers; recurrent inputs reset to zero. '
      + 'Loader is shared, not independently validated here.'
    ),
  }
  (evidence / 'source-comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  return result

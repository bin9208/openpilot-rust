from __future__ import annotations
import argparse
import hashlib
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
import types

from import_usbgpu_hcq import Exporter
from usbgpu_hcq_data import Artifact, Record


class View:
  def __init__(self, data, offset=0, size=None):
    self.data = memoryview(data)[offset:offset + size if size is not None else None]

  def view(self, offset=0, size=None, fmt=None):
    return View(self.data, offset, size)

  def __setitem__(self, index, value):
    self.data[index] = value


def original(args, bundle, snapshot):
  os.environ['DEV'] = 'CPU:LLVM'
  sys.path.insert(0, str(args.runtime))
  from tinygrad.device import Buffer, BufferStorage, Device
  from tinygrad.uop.ops import Ops
  from tinygrad.runtime.support.hcq2 import hcq_link

  addresses = snapshot['buffers']
  data = {row['index']: bytearray(row['bytes']) for row in addresses}
  buffers, memo, native_ids = {}, {}, {}
  active = None

  def allocate(size, options=None):
    if active is None or size != addresses[active]['bytes']:
      raise ValueError('original allocator size or identity mismatch')
    return BufferStorage(addresses[active]['device'], active, View(data[active]))

  def offset(pointer, size, offset):
    raise ValueError('unexpected allocator view')

  allocator = types.SimpleNamespace(alloc=allocate, _offset=offset, free=lambda *args: None)

  def get_buf(buffer, device):
    index = buffer.get_storage().meta
    return addresses[index]['host' if device == 'CPU' else 'device'] + buffer.offset

  Buffer.get_buf = get_buf

  with Artifact(args.model, args.runtime) as artifact:
    exporter = Exporter(artifact)
    exported = exporter.export()
    if exported != bundle:
      raise ValueError('descriptor differs from current importer')

    def convert(value):
      nonlocal active
      match value:
        case Record():
          if id(value) in memo:
            return memo[id(value)]
          arguments = tuple(convert(item) for item in value.arguments)
          module, name = value.global_name.rsplit('.', 1)
          constructor = getattr(importlib.import_module(module), name)
          previous = active
          if value.global_name == 'tinygrad.device.Buffer':
            active = exporter.ids[id(value)]
          result = object.__new__(constructor) if value.state is not None and not arguments else constructor(*arguments)
          active = previous
          memo[id(value)] = result
          if value.state is not None:
            for key, item in convert(value.state).items():
              object.__setattr__(result, key, item)
          if id(value) in exporter.ids:
            native_ids[id(result)] = exporter.ids[id(value)]
          return result
        case dict():
          return {convert(key): convert(item) for key, item in value.items()}
        case list():
          return [convert(item) for item in value]
        case tuple():
          return tuple(convert(item) for item in value)
        case _:
          return value

    def bufferize(node, ctx=None):
      nonlocal active
      if node.op != Ops.PARAM or node.tag is None:
        return None
      index = native_ids[id(node)]
      if index not in buffers:
        spec = bundle['buffers'][index]
        active = index
        buffer = Buffer(spec['device'], node.max_numel(), node.dtype, opaque=BufferStorage(
          addresses[index]['device'], index, View(data[index])))
        active = None
        token = {'libusb_control_transfer': 1, 'libusb_bulk_transfer': 2, 'usb_host': 1}.get(spec['tag'])
        if token:
          data[index][:8] = token.to_bytes(8, 'little')
        buffers[index] = buffer
      return buffers[index]

    fake = types.SimpleNamespace(device='AMD', allocator=allocator, pm_bufferize=types.SimpleNamespace(rewrite=bufferize))
    Device.__class__.__getitem__ = lambda self, device: fake
    run = convert(artifact.value['run'])
    linked = hcq_link(run.captured._linear, allow_cache=False)
    rows = []
    for row in addresses:
      index = row['index']
      digest = hashlib.sha256(data[index]).hexdigest()
      rows.append({'index': index, 'bytes': row['bytes'], 'source_sha256': digest, 'native_sha256': row['sha256'],
                   'exact': digest == row['sha256']})
    return {'rows': rows, 'linked_calls': len(linked.src), 'source_link_sha256': hashlib.sha256(
      (args.runtime / 'tinygrad/runtime/support/hcq2.py').read_bytes()).hexdigest()}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--runtime', type=Path, required=True)
  parser.add_argument('--model', type=Path, required=True)
  parser.add_argument('--descriptor', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  command = [str(args.binary), str(args.descriptor), str(args.model), str(args.evidence / 'native.json')]
  native = subprocess.run(command, capture_output=True, text=True, check=False)
  (args.evidence / 'native-stderr.log').write_text(native.stderr)
  if native.returncode:
    raise RuntimeError(f'native loader failed with {native.returncode}')
  bundle = json.loads(args.descriptor.read_text())
  result = original(args, bundle, json.loads((args.evidence / 'native.json').read_text()))
  result.update(invocation=command, exit_code=native.returncode, passed=all(row['exact'] for row in result['rows']))
  (args.evidence / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({key: value for key, value in result.items() if key != 'rows'}))
  assert result['passed']


if __name__ == '__main__':
  main()

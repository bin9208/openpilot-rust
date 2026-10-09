"""Retain bounded pre-dispatch backing storage for an actual stored-NaN lead."""

from __future__ import annotations

import hashlib
import json
import struct
from typing import Protocol

from usbgpu_model_oracle import virtual_bytes


class ProducerContractError(RuntimeError):
  pass


class Registers(Protocol):
  def __getitem__(self, address: int) -> int: ...


def register(values: Registers, address: int) -> int:
  """Match the source dispatcher's zero default for absent optional registers."""
  try:
    return values[address]
  except KeyError:
    return 0


class Producer:
  def __init__(self, args, snapshot):
    self.args = args
    manifest = json.loads(args.bundle.read_text())
    self.buffers = [{**row, 'bytes': manifest['buffers'][row['index']]['bytes']} for row in snapshot['buffers']]
    addresses = {row['index']: row['device'] for row in self.buffers}
    self.pointers = {
      addresses[patch['view']['buffer']] + patch['view']['offset']
      for patch in manifest['patches']
      if patch['kind'] == 'word' and patch['bytes'] == 8 and patch['value']['kind'] == 'address' and patch['value']['space'] == 'device'
    }
    self.buffers.extend(
      {'index': binding['name'], 'device': binding['address'], 'bytes': binding['bytes']}
      for binding in snapshot['bindings']
      if not any(row['device'] == binding['address'] for row in self.buffers)
    )

  def allocation(self, pointer):
    return next((row for row in self.buffers if row['device'] <= pointer < row['device'] + row['bytes']), None)

  def before(self, queue, count):
    from test.mockgpu.amd.amdgpu import (
      regCOMPUTE_NUM_THREAD_X,
      regCOMPUTE_PGM_LO,
      regCOMPUTE_PGM_RSRC2,
      regCOMPUTE_TMPRING_SIZE,
      regCOMPUTE_USER_DATA_0,
    )

    gpu = queue.gpu
    program = (gpu.regs[regCOMPUTE_PGM_LO] | gpu.regs[regCOMPUTE_PGM_LO + 1] << 32) << 8
    address = gpu.regs[regCOMPUTE_USER_DATA_0] | gpu.regs[regCOMPUTE_USER_DATA_0 + 1] << 32
    argument_bytes = virtual_bytes(gpu, address, 128)
    allocations, views = {}, []
    shader = self.allocation(program)
    if shader is None:
      raise ProducerContractError('program is outside declared backing storage')
    allocations[shader['device']] = shader
    for index, (pointer,) in enumerate(struct.iter_unpack('<Q', argument_bytes)):
      if address + index * 8 not in self.pointers:
        continue
      backing = self.allocation(pointer)
      if backing is None:
        raise ProducerContractError(f'pointer argument {index} is outside declared backing storage')
      allocations[backing['device']] = backing
      views.append({'argument': index, 'pointer': pointer, 'allocation': backing['index'], 'offset': pointer - backing['device']})
    if sum(row['bytes'] for row in allocations.values()) > 104 << 20:
      raise ProducerContractError('pre-dispatch backing storage exceeds transient 104 MiB limit')
    rsrc2 = gpu.regs[regCOMPUTE_PGM_RSRC2]
    scratch = ((register(gpu.regs, regCOMPUTE_TMPRING_SIZE) >> 12) & 0x3FFF) * (16 if gpu.arch == 'cdna' else 4)
    row = {
      'program': program,
      'program_allocation': shader['index'],
      'program_offset': program - shader['device'],
      'program_bytes': shader['device'] + shader['bytes'] - program,
      'arguments_pointer': address,
      'arguments_hex': argument_bytes.hex(),
      'views': views,
      'local': [gpu.regs[regCOMPUTE_NUM_THREAD_X + index] for index in range(3)],
      'dispatch_words': [queue.queue[(queue.rptr[0] + index) % (queue.size // 4)] for index in range(count + 1)],
      'rsrc2': rsrc2,
      'scratch_size': scratch,
      'arch': gpu.arch,
      'user_data': [register(gpu.regs, regCOMPUTE_USER_DATA_0 + index) for index in range((rsrc2 >> 1) & 0x1F)],
    }
    images = [(backing, virtual_bytes(gpu, backing['device'], backing['bytes'])) for backing in allocations.values()]
    return row, images

  def persist(self, queue, row, images, output):
    data = virtual_bytes(queue.gpu, output['pointer'], output['declared_bytes'])
    if sum(len(image) for _, image in images) + len(data) > 120 << 20:
      raise ProducerContractError('producer backing storage and output exceed 120 MiB evidence budget')
    receipts = []
    for backing, image in images:
      name = f'producer-allocation-{backing["index"]}.bin'
      (self.args.evidence / name).write_bytes(image)
      receipts.append({**backing, 'file': name, 'sha256': hashlib.sha256(image).hexdigest()})
    (self.args.evidence / 'producer-output.bin').write_bytes(data)
    row.update(
      {
        'before': receipts,
        'output': {
          **{key: value for key, value in output.items() if key != 'data'},
          'file': 'producer-output.bin',
          'bytes': len(data),
          'sha256': hashlib.sha256(data).hexdigest(),
        },
        'scope': 'Complete aliased backing allocations before actual dispatch, exact program/registers, and complete declared argument-zero output. '
        + 'NaN detection sampled only the first 4 KiB of declared argument-zero storage.',
      }
    )
    (self.args.evidence / 'producer-capture.json').write_text(json.dumps(row, indent=2) + '\n')

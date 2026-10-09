from __future__ import annotations
import hashlib
import json
import struct

from usbgpu_model_oracle import virtual_bytes


class CapturedPrefix(RuntimeError):
  pass


class Capture:
  def __init__(self, args, boundary, snapshot):
    self.args, self.boundary, self.snapshot = args, boundary, snapshot
    self.targets = {int(value) for value in args.capture_kernels.split(',')}
    self.manifest = json.loads(args.bundle.read_text())
    self.buffers = [{**value, 'bytes': self.manifest['buffers'][value['index']]['bytes']} for value in snapshot['buffers']]
    self.rows, self.bytes = [], 0

  def record(self, queue):
    from test.mockgpu.amd.amdgpu import regCOMPUTE_PGM_LO, regCOMPUTE_USER_DATA_0, regCOMPUTE_NUM_THREAD_X

    gpu = queue.gpu
    program = (gpu.regs[regCOMPUTE_PGM_LO] | gpu.regs[regCOMPUTE_PGM_LO + 1] << 32) << 8
    args_pointer = gpu.regs[regCOMPUTE_USER_DATA_0] | gpu.regs[regCOMPUTE_USER_DATA_0 + 1] << 32
    arguments = virtual_bytes(gpu, args_pointer, 128)
    views = []
    for index, (pointer,) in enumerate(struct.iter_unpack('<Q', arguments)):
      if pointer == 0:
        continue
      allocation = next((b for b in self.buffers if b['device'] <= pointer < b['device'] + b['bytes']), None)
      if allocation is not None:
        views.append({'argument': index, 'pointer': pointer, 'allocation': allocation['index'], 'offset': pointer - allocation['device']})
    allocation = next((b for b in self.buffers if b['device'] == program), None)
    return {'kernel': self.boundary.kernels, 'program': program, 'program_allocation': allocation['index'] if allocation else None,
            'arguments_pointer': args_pointer, 'arguments_hex': arguments.hex(), 'views': views,
            'local': [gpu.regs[regCOMPUTE_NUM_THREAD_X + index] for index in range(3)]}

  def capture(self, queue, row, stage):
    allocations = sorted({view['allocation'] for view in row['views']} | {row['program_allocation']})
    receipt = []
    for index in allocations:
      allocation = next(b for b in self.buffers if b['index'] == index)
      if self.bytes + allocation['bytes'] > 192 << 20:
        raise RuntimeError('full dynamic allocation capture exceeds bounded 192 MiB budget')
      data = virtual_bytes(queue.gpu, allocation['device'], allocation['bytes'])
      name = f'kernel-{row["kernel"]}-{stage}-allocation-{index}.bin'
      (self.args.evidence / name).write_bytes(data)
      self.bytes += len(data)
      receipt.append({**allocation, 'file': name, 'sha256': hashlib.sha256(data).hexdigest()})
    row[stage] = receipt

  def install(self):
    from test.mockgpu.amd.amdgpu import PM4Executor

    dispatch = PM4Executor._exec_dispatch_direct

    def capture(queue, words):
      row = self.record(queue)
      row['dispatch_words'] = [queue.queue[(queue.rptr[0] + index) % (queue.size // 4)] for index in range(words + 1)]
      if row['kernel'] in self.targets:
        self.capture(queue, row, 'before')
      result = dispatch(queue, words)
      if row['kernel'] in self.targets:
        self.capture(queue, row, 'after')
      self.rows.append(row)
      if row['kernel'] in self.targets:
        receipt = {'dispatches': self.rows, 'bytes_captured': self.bytes, 'snapshot': self.snapshot,
                   'scope': 'Actual dispatch order and complete declared argument allocations; raw bytes do not imply arena element types.'}
        (self.args.evidence / 'dispatch-capture.json').write_text(json.dumps(receipt, indent=2) + '\n')
      if row['kernel'] == max(self.targets):
        raise CapturedPrefix('requested native model prefix captured; no final inference acceptance')
      return result

    PM4Executor._exec_dispatch_direct = capture

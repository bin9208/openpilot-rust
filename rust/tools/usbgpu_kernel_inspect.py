from __future__ import annotations
import hashlib
import json
import math
import struct

from import_usbgpu_hcq import Exporter
from usbgpu_hcq_data import Artifact
from usbgpu_hcq_dispatch import dtype
from usbgpu_model_oracle import virtual_bytes


class FirstInvalidKernel(RuntimeError):
  pass


class Inspector:
  def __init__(self, args, boundary, snapshot):
    self.args, self.boundary, self.rows = args, boundary, []
    self.buffers = []
    runtime = args.model.parent / 'egpu-runtime'
    with Artifact(args.model, runtime) as artifact:
      exporter = Exporter(artifact)
      manifest = exporter.export()
      allocations = {index: item for item in artifact.records if item.global_name == 'tinygrad.device.Buffer'
                     for index in [exporter.ids[id(item)]]}
      for row in snapshot['buffers']:
        if row['index'] not in allocations:
          continue
        scalar = dtype(allocations[row['index']].arguments[2])
        if scalar[3] in ('f', 'e'):
          self.buffers.append({**row, 'bytes': manifest['buffers'][row['index']]['bytes'], 'dtype': scalar[3]})

  def arguments(self, queue):
    from test.mockgpu.amd.amdgpu import regCOMPUTE_USER_DATA_0

    address = queue.gpu.regs[regCOMPUTE_USER_DATA_0] | queue.gpu.regs[regCOMPUTE_USER_DATA_0 + 1] << 32
    rows = []
    for index, (pointer,) in enumerate(struct.iter_unpack('<Q', virtual_bytes(queue.gpu, address, 128))):
      spec = next((b for b in self.buffers if b['device'] <= pointer < b['device'] + b['bytes']), None)
      if spec is None:
        continue
      count = min(4096, spec['device'] + spec['bytes'] - pointer)
      size = struct.calcsize(spec['dtype'])
      data = virtual_bytes(queue.gpu, pointer, count - count % size)
      rows.append({'argument': index, 'pointer': pointer, 'allocation': spec['index'], 'dtype': spec['dtype'],
                   'sample_bytes': len(data), 'finite': sum(math.isfinite(v[0]) for v in struct.iter_unpack('<' + spec['dtype'], data)),
                   'values': len(data) // size, 'sha256': hashlib.sha256(data).hexdigest(), 'data': data})
    return rows

  def install(self):
    from test.mockgpu.amd.amdgpu import PM4Executor, regCOMPUTE_PGM_LO

    dispatch = PM4Executor._exec_dispatch_direct

    def inspect(queue, words):
      before = self.arguments(queue)
      result = dispatch(queue, words)
      after = self.arguments(queue)
      index = self.boundary.kernels
      row = {'kernel': index, 'program': (queue.gpu.regs[regCOMPUTE_PGM_LO] | queue.gpu.regs[regCOMPUTE_PGM_LO + 1] << 32) << 8,
             'before': [{key: value for key, value in item.items() if key != 'data'} for item in before],
             'after': [{key: value for key, value in item.items() if key != 'data'} for item in after]}
      self.rows.append(row)
      (self.args.evidence / 'kernel-inspection.json').write_text(json.dumps(self.rows, indent=2) + '\n')
      failed = [item for item in after if item['argument'] == 0 and item['finite'] != item['values']]
      if failed:
        for stage, values in [('before', before), ('after', after)]:
          for item in values:
            (self.args.evidence / f'kernel-{index}-argument-{item["argument"]}-{stage}.bin').write_bytes(item['data'])
        (self.args.evidence / 'first-invalid.json').write_text(json.dumps(row, indent=2) + '\n')
        raise FirstInvalidKernel(f'kernel {index} returned nonfinite typed output values')
      return result

    PM4Executor._exec_dispatch_direct = inspect

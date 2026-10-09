from __future__ import annotations
import hashlib
import json
import math
import struct

from import_usbgpu_hcq import Exporter
from usbgpu_hcq_data import Artifact
from usbgpu_hcq_dispatch import dtype
from usbgpu_kernel_producer import Producer
from usbgpu_kernel_types import typed_words
from usbgpu_model_oracle import virtual_bytes


class FirstInvalidKernel(RuntimeError):
  pass


class InspectionContractError(RuntimeError):
  pass


class Inspector:
  def __init__(self, args, boundary, snapshot):
    self.args, self.boundary, self.rows = args, boundary, []
    self.producer = Producer(args, snapshot)
    self.buffers, self.words = [], {}
    self.output = next(binding for binding in snapshot['bindings'] if binding['name'] == 'outputs' and binding['output'])
    runtime = args.model.parent / 'egpu-runtime'
    with Artifact(args.model, runtime) as artifact:
      exporter = Exporter(artifact)
      manifest = exporter.export()
      addresses = {row['index']: row['device'] for row in snapshot['buffers']}
      for word in typed_words(exporter):
        source_size = manifest['buffers'][word.source.buffer]['bytes']
        target_size = manifest['buffers'][word.target.buffer]['bytes']
        if not 0 < word.bytes <= source_size - word.source.offset or not 0 <= word.target.offset <= target_size - 8:
          raise InspectionContractError('declared kernel argument view exceeds allocation')
        target = addresses[word.target.buffer] + word.target.offset
        if target in self.words:
          raise InspectionContractError('ambiguous declared kernel argument word')
        self.words[target] = {
          'pointer': addresses[word.source.buffer] + word.source.offset,
          'index': word.source.buffer,
          'dtype': word.format,
          'bytes': word.bytes,
        }
      allocations = {index: item for item in artifact.records if item.global_name == 'tinygrad.device.Buffer' for index in [exporter.ids[id(item)]]}
      for row in snapshot['buffers']:
        if row['index'] not in allocations:
          continue
        scalar = dtype(allocations[row['index']].arguments[2])
        if scalar[3] in ('f', 'e'):
          self.buffers.append({**row, 'bytes': manifest['buffers'][row['index']]['bytes'], 'dtype': scalar[3]})
      output = next(binding for binding in manifest['bindings'] if binding['name'] == 'outputs' and binding['output'])
      if output['dtype'] != 'float32' or output['bytes'] != self.output['bytes']:
        raise InspectionContractError('runtime output binding does not match original f32 output specification')
      coverage = {
        'typed_words': len(self.words),
        'declared_formats': sorted({word['dtype'] for word in self.words.values()}),
        'known_f32_output': self.output,
        'original_output_specification': output,
        'sample_limit_bytes': 4096,
        'scope': 'Original GETADDR scalar and SHRINK extent per kernarg word; no byte-arena dtype inference.',
      }
      (args.evidence / 'inspection-coverage.json').write_text(json.dumps(coverage, indent=2) + '\n')

  def arguments(self, queue):
    from test.mockgpu.amd.amdgpu import regCOMPUTE_USER_DATA_0

    address = queue.gpu.regs[regCOMPUTE_USER_DATA_0] | queue.gpu.regs[regCOMPUTE_USER_DATA_0 + 1] << 32
    rows = []
    for index, (pointer,) in enumerate(struct.iter_unpack('<Q', virtual_bytes(queue.gpu, address, 128))):
      spec = self.words.get(address + index * 8)
      if spec is not None:
        if spec['pointer'] != pointer:
          raise InspectionContractError(f'kernel argument {index} does not match its original declared GETADDR')
        count = min(4096, spec['bytes'])
        declared = spec['bytes']
      else:
        spec = next((b for b in self.buffers if b['device'] <= pointer < b['device'] + b['bytes']), None)
        count = min(4096, spec['device'] + spec['bytes'] - pointer) if spec is not None else 0
        declared = spec['device'] + spec['bytes'] - pointer if spec is not None else 0
      if spec is None:
        continue
      size = struct.calcsize(spec['dtype'])
      data = virtual_bytes(queue.gpu, pointer, count - count % size)
      rows.append(
        {
          'argument': index,
          'pointer': pointer,
          'allocation': spec['index'],
          'dtype': spec['dtype'],
          'sample_bytes': len(data),
          'declared_bytes': declared,
          'finite': sum(math.isfinite(v[0]) for v in struct.iter_unpack('<' + spec['dtype'], data)),
          'nan': sum(math.isnan(v[0]) for v in struct.iter_unpack('<' + spec['dtype'], data)),
          'values': len(data) // size,
          'sha256': hashlib.sha256(data).hexdigest(),
          'data': data,
        }
      )
    return rows

  def install(self):
    from test.mockgpu.amd.amdgpu import PM4Executor, regCOMPUTE_PGM_LO

    dispatch = PM4Executor._exec_dispatch_direct

    def inspect(queue, words):
      before = self.arguments(queue)
      capture, images = self.producer.before(queue, words)
      if not self.rows:
        first = next(item for item in before if item['argument'] == 0)
        if first['dtype'] != 'e' or first['sample_bytes'] != 4096:
          raise InspectionContractError('first dispatch half argument-zero coverage is missing')
        output = virtual_bytes(queue.gpu, self.output['address'], min(4096, self.output['bytes']))
        observed = {
          'argument_zero': {key: value for key, value in first.items() if key != 'data'},
          'known_f32_output_sample_bytes': len(output),
          'known_f32_output_sha256': hashlib.sha256(output).hexdigest(),
          'known_f32_output_finite': sum(math.isfinite(value[0]) for value in struct.iter_unpack('<f', output)),
        }
        (self.args.evidence / 'inspection-first-dispatch.json').write_text(json.dumps(observed, indent=2) + '\n')
      result = dispatch(queue, words)
      after = self.arguments(queue)
      index = self.boundary.kernels
      row = {
        'kernel': index,
        'program': (queue.gpu.regs[regCOMPUTE_PGM_LO] | queue.gpu.regs[regCOMPUTE_PGM_LO + 1] << 32) << 8,
        'before': [{key: value for key, value in item.items() if key != 'data'} for item in before],
        'after': [{key: value for key, value in item.items() if key != 'data'} for item in after],
      }
      self.rows.append(row)
      (self.args.evidence / 'kernel-inspection.json').write_text(json.dumps(self.rows, indent=2) + '\n')
      failed = [item for item in after if item['argument'] == 0 and item['nan']]
      if failed:
        self.producer.persist(queue, {**capture, 'kernel': index}, images, failed[0])
        for stage, values in [('before', before), ('after', after)]:
          for item in values:
            (self.args.evidence / f'kernel-{index}-argument-{item["argument"]}-{stage}.bin').write_bytes(item['data'])
        (self.args.evidence / 'first-invalid.json').write_text(json.dumps(row, indent=2) + '\n')
        raise FirstInvalidKernel(f'kernel {index} has NaN in sampled declared argument-zero storage; localization lead only')
      return result

    PM4Executor._exec_dispatch_direct = inspect

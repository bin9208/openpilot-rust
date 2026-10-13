"""Build-time import of compiled HCQ2 artifacts; output contains no executable Python."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

from usbgpu_hcq_data import Artifact, ArtifactError, Json, Record, record, state
from usbgpu_hcq_dispatch import dispatcher, dtype, sources


class Exporter:
  def __init__(self, artifact: Artifact):
    self.artifact = artifact
    self.buffers: list[Json] = []
    self.ids: dict[int, int] = {}
    for item in artifact.records:
      if item.global_name == 'tinygrad.device.Buffer':
        if len(item.arguments) != 6:
          raise ArtifactError('unsupported serialized Buffer constructor')
        device, count, scalar, opaque, options, initial = item.arguments
        if device != 'AMD' or opaque is not None or not isinstance(count, int):
          raise ArtifactError('only owned AMD artifact buffers are supported')
        self.ids[id(item)] = len(self.buffers)
        self.buffers.append(
          {
            'kind': 'allocation',
            'bytes': count * ((dtype(scalar)[1] + 7) // 8),
            'uncached': state(options)['uncached'],
            'host': state(options)['host'],
            'cpu_access': state(options)['cpu_access'],
            'initial': artifact.blob(initial),
          }
        )

  def view(self, node: Record) -> dict[str, Json]:
    op, src = self.artifact.op(node), sources(node)
    match op:
      case 'BITCAST':
        return self.view(src[0])
      case 'SHRINK':
        view = self.view(src[0])
        view['offset'] += self.constant(src[1]) * self.width(src[0])
        return view
      case 'BUFFER':
        return {'buffer': self.ids[id(state(node.arguments[2])['buffer'])], 'offset': 0}
      case 'PARAM':
        if id(node) not in self.ids:
          info, tag = state(node.arguments[2]), node.arguments[3]
          if tag is None:
            raise ArtifactError('unbound external parameter in static arguments')
          self.ids[id(node)] = len(self.buffers)
          self.buffers.append(
            {
              'kind': 'placeholder',
              'tag': tag[2] if isinstance(tag, tuple) and tag[:2] == ('cfunc', 'libusb') else tag,
              'bytes': info['size'] * self.width(node),
              'elements': info['size'],
              'host': info['volatile'],
              'cpu_access': True,
              'uncached': info['volatile'],
              'device': info['device'][0] if isinstance(info['device'], tuple) and len(info['device']) == 1 else info['device'],
            }
          )
        return {'buffer': self.ids[id(node)], 'offset': 0}
      case _:
        raise ArtifactError(f'unsupported buffer view: {op}')

  def constant(self, node: Record) -> int:
    if self.artifact.op(node) != 'CONST' or not isinstance(node.arguments[2], int):
      raise ArtifactError('link offset is not a constant integer')
    return node.arguments[2]

  def width(self, node: Record) -> int:
    match self.artifact.op(node):
      case 'PARAM' | 'BUFFER':
        return (dtype(state(node.arguments[2])['dtype'])[1] + 7) // 8
      case 'BITCAST':
        return (dtype(node.arguments[2])[1] + 7) // 8
      case 'SHRINK':
        return self.width(sources(node)[0])
      case 'GETADDR' | 'ADD' | 'SHR':
        return 8
      case _:
        raise ArtifactError('unsupported link value width')

  def expression(self, node: Record) -> dict[str, Json]:
    op, src = self.artifact.op(node), sources(node)
    match op:
      case 'GETADDR':
        device = node.arguments[2]
        if device not in ('CPU', ('AMD',)):
          raise ArtifactError(f'unsupported address space: {device}')
        return {'kind': 'address', 'view': self.view(src[0]), 'space': 'host' if device == 'CPU' else 'device'}
      case 'CONST':
        return {'kind': 'constant', 'value': self.constant(node)}
      case 'ADD' | 'SHR':
        return {'kind': op.lower(), 'left': self.expression(src[0]), 'right': self.expression(src[1])}
      case _:
        raise ArtifactError(f'unsupported link expression: {op}')

  def patches(self, root: Record) -> list[Json]:
    visited, ordered = set(), []

    def visit(node: Record) -> None:
      if id(node) in visited or self.artifact.op(node) == 'PROGRAM':
        return
      visited.add(id(node))
      for source in sources(node):
        visit(source)
      ordered.append(node)

    visit(root)
    patches: list[Json] = []
    for node in ordered:
      if self.artifact.op(node) != 'STORE':
        continue
      target, value = sources(node)
      if self.artifact.op(target) == 'INDEX':
        base, offsets = sources(target)
        if self.artifact.op(offsets) != 'STACK' or self.artifact.op(value) != 'STACK':
          raise ArtifactError('unsupported link scalar patch')
        indices, words = sources(offsets), sources(value)
        if len(indices) != len(words):
          raise ArtifactError('link patch arity mismatch')
        for offset, word in zip(indices, words, strict=True):
          view = self.view(base)
          width = self.width(word)
          view['offset'] += self.constant(offset) * width
          patches.append({'kind': 'word', 'view': view, 'bytes': width, 'value': self.expression(word)})
      else:
        if self.artifact.op(value) == 'BITCAST':
          value = sources(value)[0]
        if self.artifact.op(value) != 'BINARY':
          raise ArtifactError('unsupported link byte patch')
        patches.append({'kind': 'blob', 'view': self.view(target), 'blob': self.artifact.blob(value.arguments[2])})
    return patches

  def export(self) -> dict[str, Json]:
    root = self.artifact.value
    if not isinstance(root, dict):
      raise ArtifactError('expected model dictionary')
    captured = record(record(root['run'], '._TinyJit').arguments[1], '.CapturedJit')
    linear = record(captured.arguments[1], '.UOp')
    call = sources(sources(linear)[0])[0]
    if self.artifact.op(call) != 'CALL' or self.artifact.op(sources(call)[0]) != 'PROGRAM':
      raise ArtifactError('expected one HCQ host program call')
    program = sources(call)[0]
    info = state(record(call.arguments[2], '.CallInfo').arguments[4])
    patches = self.patches(linear)
    arguments = [self.view(node) for node in sources(call)[1:]]
    nodes, parameters = dispatcher(self.artifact, program)
    inputs, outputs = root['input_specs'], root['output_specs']
    if not isinstance(inputs, dict) or not isinstance(outputs, dict):
      raise ArtifactError('model input/output specifications missing')
    names = captured.arguments[2]
    if not isinstance(names, list) or set(names) != set(inputs):
      raise ArtifactError('captured input names do not match model')
    bindings: list[Json] = []
    for name, spec, output in [(name, inputs[name], False) for name in names] + [(name, spec, True) for name, spec in outputs.items()]:
      shape, scalar, device = spec
      if device != 'AMD' or scalar not in ('|u1', 'uint8', '<f4', 'float32'):
        raise ArtifactError('unsupported model tensor specification')
      alias = names.index(name.removeprefix('next_')) if output and name.startswith('next_state_') and name.removeprefix('next_') in names else None
      bindings.append(
        {
          'name': name,
          'shape': list(shape),
          'bytes': math.prod(shape) * (1 if scalar in ('|u1', 'uint8') else 4),
          'dtype': 'uint8' if scalar in ('|u1', 'uint8') else 'float32',
          'output': output,
          'alias': alias,
        }
      )
    table = []
    for value, device, offset in info['inputs']:
      if device != 'AMD' or self.artifact.op(value) != 'PARAM':
        raise ArtifactError('unsupported dynamic input address')
      table.append({'slot': state(value.arguments[2])['slot'], 'offset': offset})
    return {
      'version': 1,
      'model_sha256': self.artifact.sha256,
      'model_bytes': self.artifact.path.stat().st_size,
      'buffers': self.buffers,
      'patches': patches,
      'arguments': arguments,
      'bindings': bindings,
      'input_table': {'argument': info['table'], 'entries': table},
      'dispatcher': nodes,
      'parameters': parameters,
      'kernel_count': len(info['kernels']),
    }


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--model', type=Path, required=True)
  parser.add_argument('--runtime', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--worker-metadata', type=Path)
  args = parser.parse_args()
  with Artifact(args.model, args.runtime) as artifact:
    bundle = Exporter(artifact).export()
    if args.worker_metadata is not None:
      from usbgpu_worker_metadata import export

      args.worker_metadata.write_text(json.dumps(export(artifact), separators=(',', ':')) + '\n')
  args.output.write_text(json.dumps(bundle, separators=(',', ':')) + '\n')
  print(json.dumps({'buffers': len(bundle['buffers']), 'patches': len(bundle['patches']), 'kernel_count': bundle['kernel_count']}))


if __name__ == '__main__':
  main()

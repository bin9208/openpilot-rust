"""Lower the artifact's integer USB host dispatcher to the native owned-memory VM.

Semantics follow tinygrad's dtype_from_uop and C renderer; model code remains data.
"""

from __future__ import annotations

from usbgpu_hcq_data import Artifact, ArtifactError, Json, Record, Value, record, state


def sources(node: Record) -> tuple[Record, ...]:
  values = node.arguments[1]
  if not isinstance(values, tuple):
    raise ArtifactError('UOp sources must be a tuple')
  return tuple(record(value, '.UOp') for value in values)


def dtype(value: Value) -> tuple[int, int, str, str | None]:
  arguments = record(value, '.DType').arguments
  if len(arguments) != 4:
    raise ArtifactError('unsupported DType arguments')
  rank, bits, name, fmt = arguments
  if not isinstance(rank, int) or not isinstance(bits, int) or not isinstance(name, str) or not (fmt is None or isinstance(fmt, str)):
    raise ArtifactError('invalid DType fields')
  return rank, bits, name, fmt


def dispatcher(artifact: Artifact, program: Record) -> tuple[list[Json], list[Json]]:
  linear = sources(sources(program)[1])
  ids = {id(node): index for index, node in enumerate(linear)}
  types, pointers, nodes, parameters = [], [], [], []
  for node in linear:
    op, arg = artifact.op(node), node.arguments[2]
    src = [ids[id(source)] for source in sources(node)]
    pointer = op in {'PARAM', 'BUFFER'} or op in {'INDEX', 'SHRINK', 'AFTER', 'BITCAST'} and pointers[src[0]]
    match op:
      case 'PARAM' | 'BUFFER':
        scalar = dtype(state(arg)['dtype'])
      case 'CAST' | 'BITCAST':
        scalar = dtype(arg)
      case 'CALL':
        scalar = dtype(record(arg, '.CallInfo').arguments[5])
      case 'CMPLT' | 'CMPNE' | 'CMPEQ':
        scalar = (0, 1, 'bool', '?')
      case 'CONST':
        if not isinstance(arg, int):
          raise ArtifactError('host dispatcher constants must be integer')
        scalar = (0, 1, 'bool', '?') if isinstance(arg, bool) else (0, 64, 'weakint', 'q')
      case 'INDEX' | 'SHRINK' | 'AFTER' | 'LOAD' | 'RANGE' | 'SHR' | 'SHL':
        scalar = types[src[0]]
      case 'STORE':
        scalar = types[src[1]]
      case 'ADD' | 'SUB' | 'MUL' | 'AND' | 'OR' | 'XOR':
        scalar = max((types[s] for s in src), key=lambda x: x[0])
      case 'WHERE':
        scalar = max((types[s] for s in src[1:]), key=lambda x: x[0])
      case 'CUSTOM_FUNCTION' | 'END' | 'SINK' | 'BARRIER' | 'NOOP':
        scalar = (-1, 0, 'void', None)
      case _:
        raise ArtifactError(f'unsupported host dispatcher operation: {op}')
    types.append(scalar)
    pointers.append(pointer)
    operation: dict[str, Json] = {
      'kind': {
        'PARAM': 'argument',
        'BUFFER': 'local',
        'CONST': 'constant',
        'INDEX': 'index',
        'SHRINK': 'index',
        'CMPLT': 'less',
        'CMPNE': 'not_equal',
        'CMPEQ': 'equal',
        'CUSTOM_FUNCTION': 'function',
        'SINK': 'noop',
        'BARRIER': 'noop',
        'NOOP': 'noop',
      }.get(op, op.lower())
    }
    match op:
      case 'PARAM':
        info = state(arg)
        operation['slot'] = info['slot']
        parameters.append({'slot': info['slot'], 'name': info['name'], 'bytes': info['size'] * ((scalar[1] + 7) // 8)})
      case 'BUFFER':
        operation['bytes'] = state(arg)['size'] * ((scalar[1] + 7) // 8)
      case 'CONST':
        operation['value'] = int(arg) & ((1 << 64) - 1)
      case 'INDEX' | 'SHRINK':
        operation['stride'] = (types[src[0]][1] + 7) // 8
      case 'CUSTOM_FUNCTION':
        if arg not in {'libusb_control_transfer', 'libusb_bulk_transfer'}:
          raise ArtifactError(f'unsupported dispatcher function: {arg}')
        operation['function'] = {'libusb_control_transfer': 'control', 'libusb_bulk_transfer': 'bulk'}[arg]
    nodes.append({'op': operation, 'src': src, 'bits': scalar[1], 'signed': scalar[3] in {'b', 'h', 'i', 'q'}, 'pointer': pointer})
  return nodes, parameters

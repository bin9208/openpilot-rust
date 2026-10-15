"""Data-only lowering of the pinned persistent-buffer HCQ host ABI.

Nested LINEAR calls are statically inlined with a fresh parameter/local scope.
The Rust VM owns all memory and still polls its deadline before every operation.
"""
from usbgpu_hcq_data import ArtifactError, Record, record, state
from usbgpu_hcq_dispatch import dtype, sources

VOID = (-1, 0, 'void', None)


def scalar_parameter(node):
  return record(state(node.arguments[2])['addrspace'], '.AddrSpace').arguments == (4,)


def count(node):
  values = sources(node)
  if len(values) != 1 or sources(values[0]) or values[0].arguments[2] is None:
    raise ArtifactError('HCQ buffer needs one constant extent')
  value = values[0].arguments[2]
  if type(value) is not int or value < 0:
    raise ArtifactError('HCQ buffer extent is not a nonnegative integer')
  return value


class Lowerer:
  def __init__(self, artifact):
    self.artifact = artifact
    self.nodes, self.types, self.pointers, self.parameters = [], [], [], []

  def emit(self, operation, src, scalar, pointer=False):
    index = len(self.nodes)
    if index >= 65536:
      raise ArtifactError('HCQ flattened dispatcher node limit')
    self.nodes.append({'op': operation, 'src': src, 'bits': scalar[1],
                       'signed': scalar[3] in {'b', 'h', 'i', 'q'}, 'pointer': pointer})
    self.types.append(scalar)
    self.pointers.append(pointer)
    return index

  def body(self, linear, arguments=None, depth=0):
    if depth > 16:
      raise ArtifactError('HCQ nested call depth limit')
    ids = {}
    def value(node):
      if id(node) in ids:
        return ids[id(node)]
      op, arg = self.artifact.op(node), node.arguments[2]
      operands = sources(node)
      if op == 'PARAM':
        info = state(arg)
        scalar, pointer = dtype(info['dtype']), not scalar_parameter(node)
        slot = info['slot']
        if arguments is not None:
          if type(slot) is not int or slot < 0 or slot >= len(arguments):
            raise ArtifactError('HCQ nested parameter slot')
          result = arguments[slot]
          if self.types[result] != scalar or self.pointers[result] != pointer:
            raise ArtifactError('HCQ nested parameter type mismatch')
        else:
          result = self.emit({'kind': 'argument', 'slot': slot}, [], scalar, pointer)
          self.parameters.append({'slot': slot, 'name': info['name'] or f'arg_{slot}',
                                  'bytes': count(node) * ((scalar[1]+7)//8) if pointer else 0})
        ids[id(node)] = result
        return result
      if op == 'CALL' and self.artifact.op(operands[0]) == 'LINEAR':
        args = [value(n) for n in operands[1:]]
        self.body(operands[0], args, depth+1)
        result = self.emit({'kind':'noop'}, [], VOID)
        ids[id(node)] = result
        return result
      if op == 'CALL' and self.artifact.op(operands[0]) == 'CUSTOM_FUNCTION':
        info = state(operands[0].arguments[2])
        name = info['name']
        args = [value(n) for n in operands[1:]]
        if name == 'clock_gettime':
          operation, src = {'kind':'clock_gettime'}, args
        elif name in {'libusb_control_transfer', 'libusb_bulk_transfer'}:
          token = 1 if name == 'libusb_control_transfer' else 2
          constant = self.emit({'kind':'constant','value':token}, [], (8,64,'u64','Q'))
          function = self.emit({'kind':'function','function':'control' if token == 1 else 'bulk'}, [constant], VOID)
          operation, src = {'kind':'call'}, [function]+args
        else:
          raise ArtifactError(f'unsupported HCQ custom call: {name}')
        result = self.emit(operation, src, dtype(info['dtype']))
        ids[id(node)] = result
        return result
      # Buffer extents, parameter dimensions, and call bodies are metadata,
      # not runtime dependencies. Avoid entering them in the containing scope.
      if op in {'CONST','BUFFER','BINARY','STACK','CUSTOM_FUNCTION'}:
        src = []
      else:
        src = [value(n) for n in operands]
      pointer = op == 'BUFFER' or op in {'INDEX','SHRINK','AFTER','BITCAST'} and self.pointers[src[0]]
      if op == 'BUFFER': scalar = dtype(state(arg)['dtype'])
      elif op in {'CAST','BITCAST'}: scalar = dtype(arg)
      elif op in {'CMPLT','CMPNE','CMPEQ'}: scalar = (0,1,'bool','?')
      elif op == 'CONST':
        if not isinstance(arg,int): raise ArtifactError('HCQ host constant is not integer')
        scalar = (0,1,'bool','?') if isinstance(arg,bool) else (0,64,'weakint','q')
      elif op in {'INDEX','SHRINK','AFTER','LOAD','RANGE','SHR','SHL','NEG','END'}: scalar = self.types[src[0]]
      elif op == 'STORE': scalar = self.types[src[1]]
      elif op in {'ADD','SUB','MUL','AND','OR','XOR','CDIV'}: scalar = max((self.types[s] for s in src), key=lambda t:t[0])
      elif op == 'WHERE': scalar = max((self.types[s] for s in src[1:]), key=lambda t:t[0])
      elif op in {'IF','ENDIF','BACKEDGE','SINK','BARRIER','NOOP','STACK'}: scalar = VOID
      elif op == 'BINARY': scalar, pointer = (2,8,'u8','B'), True
      else: raise ArtifactError(f'unsupported HCQ persistent dispatcher operation: {op}')
      operation = {'kind': {'CONST':'constant','BUFFER':'local','INDEX':'index','SHRINK':'index',
                   'CMPLT':'less','CMPNE':'not_equal','CMPEQ':'equal','BACKEDGE':'end',
                   'SINK':'noop','BARRIER':'noop','NOOP':'noop','STACK':'noop','BINARY':'data'}.get(op,op.lower())}
      if op == 'CONST': operation['value'] = int(arg) & ((1<<64)-1)
      elif op == 'BUFFER': operation['bytes'] = count(node) * ((scalar[1]+7)//8)
      elif op == 'BINARY':
        if not isinstance(arg,bytes): raise ArtifactError('HCQ inline binary must be bytes')
        operation['bytes'] = list(arg)
      elif op in {'INDEX','SHRINK'}: operation['stride'] = (self.types[src[0]][1]+7)//8
      elif op == 'IF': src = src[:1]
      result = self.emit(operation,src,scalar,pointer)
      ids[id(node)] = result
      return result
    for node in sources(linear):
      value(node)


def dispatcher(artifact, program):
  lowerer = Lowerer(artifact)
  lowerer.body(sources(program)[1])
  return lowerer.nodes, lowerer.parameters

"""Extract declarative AMD register/layout metadata without importing source modules."""

from __future__ import annotations
import argparse
import ast
import hashlib
import json
import operator
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'tinygrad_repo/tinygrad/runtime/autogen/am'
TARGET = ROOT / 'rust/crates/usbgpu/assets/amd-metadata.json'
BINARY = {
  ast.Add: operator.add,
  ast.Sub: operator.sub,
  ast.Mult: operator.mul,
  ast.FloorDiv: operator.floordiv,
  ast.LShift: operator.lshift,
  ast.RShift: operator.rshift,
  ast.BitOr: operator.or_,
  ast.BitAnd: operator.and_,
  ast.BitXor: operator.xor,
}


def integer(node, known):
  if isinstance(node, ast.Constant) and isinstance(node.value, int):
    return node.value
  if isinstance(node, ast.Name):
    return known[node.id]
  if isinstance(node, ast.BinOp) and type(node.op) in BINARY:
    return BINARY[type(node.op)](integer(node.left, known), integer(node.right, known))
  if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd, ast.Invert)):
    return {ast.USub: operator.neg, ast.UAdd: operator.pos, ast.Invert: operator.invert}[type(node.op)](integer(node.operand, known))
  raise ValueError(ast.unparse(node))


def constants(tree):
  result = {}
  for node in tree.body:
    for child in ast.walk(node):
      if isinstance(child, ast.NamedExpr) and isinstance(child.target, ast.Name):
        result[child.target.id] = integer(child.value, result)
    if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
      try:
        result[node.targets[0].id] = integer(node.value, result)
      except (KeyError, ValueError):
        pass
  return result


def selected_layout_nodes(tree, roots):
  aliases = {
    n.target.id: n
    for n in tree.body
    if isinstance(n, ast.AnnAssign) and isinstance(n.target, ast.Name) and isinstance(n.annotation, ast.Name) and n.annotation.id == 'TypeAlias'
  }
  records = {n.name: n for n in tree.body if isinstance(n, ast.ClassDef)}
  fields = {
    n.value.func.value.id: n
    for n in tree.body
    if isinstance(n, ast.Expr) and isinstance(n.value, ast.Call) and isinstance(n.value.func, ast.Attribute) and n.value.func.attr == 'register_fields'
  }
  selected, pending = {}, list(roots)
  while pending:
    name = pending.pop()
    if name in selected:
      continue
    nodes = [aliases[name]] if name in aliases else [records[name], fields[name]]
    selected[name] = nodes
    for node in nodes:
      pending.extend(ref.id for ref in ast.walk(node) if isinstance(ref, ast.Name) and ref.id != name and (ref.id in aliases or ref.id in records))
  return [node for nodes in selected.values() for node in nodes]


def macros(tree, known):
  result = {}
  for node in tree.body:
    if not isinstance(node, ast.Assign) or not isinstance(node.value, ast.Lambda) or len(node.value.args.args) != 1:
      continue
    parameter, expr = node.value.args.args[0].arg, node.value.body
    shift = 0
    if isinstance(expr, ast.BinOp) and isinstance(expr.op, ast.LShift):
      shift, expr = integer(expr.right, known), expr.left
    mask = (1 << 32) - 1
    if isinstance(expr, ast.BinOp) and isinstance(expr.op, ast.BitAnd):
      mask, expr = integer(expr.right, known), expr.left
    if isinstance(expr, ast.Name) and expr.id == parameter:
      result[node.targets[0].id] = [mask, shift]
  return result


def metadata():
  sources = {}

  def parse(name):
    path = SOURCE / name
    sources[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return ast.parse(path.read_text())

  registers = {node.targets[0].id: ast.literal_eval(node.value) for node in parse('regs.py').body if isinstance(node, ast.Assign)}
  modules = registers.pop('__all__')
  hashes = next(ast.literal_eval(node.value) for node in parse('fw.py').body if isinstance(node, ast.Assign) and node.targets[0].id == 'hashes')
  am = parse('am.py')
  hsa, kd = parse('../hsa.py'), parse('../amdgpu_kd.py')
  hsa_roots = {'struct_hsa_kernel_dispatch_packet_s', 'struct_amd_queue_s'}
  hsa_roots.update(
    node.name for node in hsa.body if isinstance(node, ast.ClassDef) and node.name.startswith(('union_COMPUTE_TMPRING_SIZE', 'union_SQ_BUF_RSRC_WORD'))
  )
  layout_nodes = am.body + selected_layout_nodes(hsa, hsa_roots) + selected_layout_nodes(kd, {'llvm_amdhsa_kernel_descriptor_t'})
  aliases, layouts = {}, {}
  for node in layout_nodes:
    if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and isinstance(node.annotation, ast.Name) and node.annotation.id == 'TypeAlias':
      aliases[node.target.id] = node.value
    if isinstance(node, ast.ClassDef):
      sizes = [
        ast.literal_eval(child.value)
        for child in node.body
        if isinstance(child, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'SIZE' for target in child.targets)
      ]
      if sizes:
        layouts[node.name] = {'size': sizes[0], 'fields': {}}

  def field_type(node):
    name = ast.unparse(node)
    if name in aliases:
      return field_type(aliases[name])
    if name in layouts:
      return {'kind': 'record', 'name': name}
    primitive = {
      'ctypes.c_uint8': 1,
      'ctypes.c_ubyte': 1,
      'ctypes.c_uint16': 2,
      'ctypes.c_uint32': 4,
      'ctypes.c_uint64': 8,
      'ctypes.c_void_p': 8,
      'ctypes.c_int32': 4,
      'ctypes.c_int64': 8,
    }
    if name in {'ctypes.c_float', 'ctypes.c_double'}:
      return {'kind': 'opaque', 'size': 4 if name == 'ctypes.c_float' else 8}
    if name in primitive:
      return {'kind': 'integer', 'size': primitive[name], 'signed': name in {'ctypes.c_int32', 'ctypes.c_int64'}}
    if isinstance(node, ast.Subscript) and ast.unparse(node.value) == 'c.POINTER':
      return {'kind': 'pointer', 'size': 8}
    if isinstance(node, ast.Subscript) and ast.unparse(node.value) == 'c.Array':
      element, length = node.slice.elts
      assert isinstance(length, ast.Subscript) and ast.unparse(length.value) == 'Literal'
      return {'kind': 'array', 'element': field_type(element), 'count': ast.literal_eval(length.slice)}
    raise ValueError(f'unsupported declarative field type {name}')

  for node in layout_nodes:
    if (
      isinstance(node, ast.Expr)
      and isinstance(node.value, ast.Call)
      and isinstance(node.value.func, ast.Attribute)
      and node.value.func.attr == 'register_fields'
    ):
      layout = layouts[node.value.func.value.id]
      for field in node.value.args[0].elts:
        name, datatype, offset, *bits = field.elts
        layout['fields'][ast.literal_eval(name)] = {
          'datatype': field_type(datatype),
          'offset': ast.literal_eval(offset),
          'bits': [ast.literal_eval(value) for value in bits],
        }
  all_constants = {'am': constants(am)}
  for name in ['soc_9', 'soc_11', 'soc_12', 'navi_offsets', 'vega_offsets', 'smu_13_0_0', 'smu_13_0_6', 'smu_13_0_12', 'smu_14_0_2']:
    all_constants[name] = constants(parse(name + '.py'))
  macro_data = {}
  for name in ['pm4_nv', 'pm4_soc15', 'sdma_4_0_0', 'sdma_5_0_0', 'sdma_6_0_0']:
    tree = parse(name + '.py')
    all_constants[name] = constants(tree)
    macro_data[name] = macros(tree, all_constants[name])
  all_constants.update(hsa=constants(hsa), amdgpu_kd=constants(kd))
  hwid = next(node.value for node in am.body if isinstance(node, ast.Assign) and node.targets[0].id == 'hw_id_map')
  hwid = {integer(key, all_constants['am']): integer(value, all_constants['am']) for key, value in zip(hwid.keys, hwid.values, strict=True)}
  return {
    'sources': sources,
    'modules': modules,
    'registers': registers,
    'firmware_hashes': hashes,
    'layouts': layouts,
    'constants': all_constants,
    'hardware_ids': hwid,
    'macros': macro_data,
  }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--check', action='store_true')
  args = parser.parse_args()
  value = metadata()
  encoded = json.dumps(value, separators=(',', ':'), sort_keys=True) + '\n'
  if args.check:
    assert TARGET.read_text() == encoded, 'AMD metadata is stale'
  else:
    if shutil.disk_usage(ROOT).free < 25 * 1024**3 + len(encoded) * 2:
      raise RuntimeError('AMD metadata generation requires 25 GiB free plus output headroom')
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_text(encoded)
  print(f'PASS {len(value["modules"])} register modules, {len(value["layouts"])} native layouts; {len(encoded)} bytes')


if __name__ == '__main__':
  main()

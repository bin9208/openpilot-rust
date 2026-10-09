"""Compare unchanged PCIe bridge/BAR setup against the native implementation."""

from __future__ import annotations
import argparse
import ast
import json
from pathlib import Path
import subprocess
import types

ROOT = Path(__file__).resolve().parents[2]


def source(case):
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/support/system.py'
  tree = ast.parse(path.read_text())
  method = next(
    node
    for item in tree.body
    if isinstance(item, ast.ClassDef) and item.name == '_System'
    for node in item.body
    if isinstance(node, ast.FunctionDef) and node.name == 'pci_setup_usb_bars'
  )
  tree.body = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0), method]
  ast.fix_missing_locations(tree)
  pci_tree = ast.parse((ROOT / 'tinygrad_repo/tinygrad/runtime/autogen/pci.py').read_text())
  required = {node.attr for node in ast.walk(method) if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id == 'pci'}
  pci_tree.body = [
    node for node in pci_tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in required for target in node.targets)
  ]
  pci = types.ModuleType('pci_fixture_constants')
  exec(compile(pci_tree, '<original PCI constants>', 'exec'), pci.__dict__)
  env = {'pci': pci, 'round_up': lambda value, alignment: (value + alignment - 1) // alignment * alignment}
  exec(compile(tree, str(path), 'exec'), env)
  values = {int(key): value for key, value in case['values'].items()}
  masks = {int(key): value for key, value in case['masks'].items()}
  trace = []

  def request(offset, bus, dev, fn, value=None, size=4):
    assert dev == fn == 0
    trace.append({'bus': bus, 'offset': offset, 'size': size, 'value': value})
    if value is not None:
      if bus == 4:
        values[offset] = value
      return None
    current = values.get(offset, 0)
    return masks.get(offset, current) if current == 0xFFFFFFFF else current

  bars = env['pci_setup_usb_bars'](None, types.SimpleNamespace(pcie_cfg_req=request), 4, 0x10000000, 32 << 30)
  return {'bars': {str(key): list(value) for key, value in bars.items()}, 'trace': trace}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', required=True, type=Path)
  args = parser.parse_args()
  cases = []
  for rebar in [False, True]:
    for vram_bits in [28, 30, 32, 34]:
      for doorbell_64 in [False, True]:
        values = {0x10: 12, 0x14: 0, 0x18: 4 if doorbell_64 else 0, 0x1C: 0 if doorbell_64 else 1, 0x20: 1, 0x24: 0}
        masks = {
          0x10: ((1 << 64) - (1 << vram_bits)) & 0xFFFFFFFF,
          0x14: (((1 << 64) - (1 << vram_bits)) >> 32),
          0x18: 0xFFFF0000,
          0x1C: 0xFFFFFFFF,
          0x24: 0xFFF00000,
        }
        if rebar:
          values.update({0x100: 0x12000001, 0x120: 0x15, 0x124: 0xFFFFFFF0, 0x128: 0xABC00007})
        cases.append({'values': {str(key): value for key, value in values.items()}, 'masks': {str(key): value for key, value in masks.items()}})
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), capture_output=True, text=True, timeout=30)
  assert process.returncode == 0, process.stderr
  results = []
  for case, line in zip(cases, process.stdout.splitlines(), strict=True):
    native = json.loads(line)
    original = source(case)
    results.append({'input': case, 'source': original, 'native': native, 'passed': original == native})
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': results}, indent=2) + '\n')
  assert all(result['passed'] for result in results)
  print(f'PASS {len(results)} original PCIe config/BAR transaction traces')


if __name__ == '__main__':
  main()

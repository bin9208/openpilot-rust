"""Compare queue C-layout fields and one-argument packet macros to original modules."""

from __future__ import annotations
import argparse
import importlib
import json
from pathlib import Path
import random
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  catalog = json.loads((ROOT / 'rust/crates/usbgpu/assets/amd-metadata.json').read_text())
  rng, requests, expected = random.Random(154), [], []
  modules = [importlib.import_module('tinygrad.runtime.autogen.' + name) for name in ['hsa', 'amdgpu_kd']]
  for record, layout in catalog['layouts'].items():
    cls = next((getattr(module, record) for module in modules if hasattr(module, record)), None)
    if cls is None:
      continue
    for name, field in layout['fields'].items():
      datatype = field['datatype']
      if datatype['kind'] != 'integer':
        continue
      width = field['bits'][0] if field['bits'] else datatype['size'] * 8
      signed = datatype.get('signed', False)
      mask = (1 << width) - 1
      for raw in [0, mask, rng.randrange(mask + 1)]:
        value = raw - (1 << width) if signed and raw & (1 << (width - 1)) else raw
        original = cls()
        setattr(original, name, value)
        data = list(bytes(original))
        requests.append({'record': record, 'field': name, 'bytes': [0] * layout['size'], 'write': raw, 'signed': signed})
        decoded = getattr(original, name)
        expected.append({'bytes': data, 'value': 0 if decoded is None else decoded})
  for module_name, macros in catalog['macros'].items():
    module = importlib.import_module('tinygrad.runtime.autogen.am.' + module_name)
    for name, (mask, shift) in macros.items():
      limit = mask & ((1 << (32 - shift)) - 1) if shift < 32 else 0
      for value in [0, limit, rng.randrange(limit + 1)]:
        requests.append({'module': module_name, 'name': name, 'value': value})
        expected.append(getattr(module, name)(value))
  command = [args.binary]
  process = subprocess.run(command, input=''.join(json.dumps(item) + '\n' for item in requests), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(actual) == len(expected)
  rows = [
    {'request': request, 'source': source, 'native': native, 'passed': source == native}
    for request, source, native in zip(requests, expected, actual, strict=True)
  ]
  args.evidence.write_text(json.dumps({'invocation': command, 'results': rows}, indent=2) + '\n')
  failures = [row for row in rows if not row['passed']]
  assert not failures, failures[:3]
  print(f'PASS {len(rows)} original HSA/kernel-descriptor and packet-macro comparisons')


if __name__ == '__main__':
  main()

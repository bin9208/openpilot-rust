"""Final read_mode over actual source Cython std::stoi for valid raw prefixes.

Malformed/range Cython values abort outside Python try/except; Rust intentionally
retains fail-closed handling there. This fixture never launches those aborts.
Logging is replaced only to isolate the binding's filesystem numeric reads.
"""
import argparse
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType, SimpleNamespace
from signal_source import classes, compare


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  binding = args.binding.resolve()
  provenance = json.loads(binding.with_name('provenance.json').read_text())
  assert hashlib.sha256(binding.read_bytes()).hexdigest() == provenance['module_sha256']
  for name, digest in {
      'openpilot/common/params.h': '47d08fa1987c9150e55d09a2a533a0a115851aadb34bd1a0c82325e0be8726b3',
      'openpilot/common/params_pyx.pyx': 'a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544',
  }.items():
    assert provenance['sources'][name] == digest
  log = ModuleType('openpilot.common.swaglog')
  log.cloudlog = SimpleNamespace(error=lambda *a, **k: None)
  sys.modules[log.__name__] = log
  spec = importlib.util.spec_from_file_location('openpilot.common.params_pyx', binding)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  scope = classes('openpilot/selfdrive/carrot/signal_mode.py', ['read_mode'])
  scope['PARAM'] = 'SignalAssistMode'
  values = [b''] + [prefix + sign + number + suffix for prefix, sign, number, suffix in itertools.product(
    (b'', b' ', b'\t', b'\n', b'\r', b'\v', b'\f', b' \t\n'),
    (b'', b'+', b'-'), (b'0', b'1', b'2', b'3', b'0001', b'0002', b'2147483647'),
    (b'', b'tail', b'.5', b'\0tail', b'\xff', b'_000'))]
  expected = []
  os.environ['OPENPILOT_PREFIX'] = 'signal_fixture'
  with tempfile.TemporaryDirectory(prefix='signal-source-mode-') as root:
    params = module.Params(root)
    for value in values:
      Path(params.get_param_path('SignalAssistMode')).write_bytes(value)
      expected.append(scope['read_mode'](params))
  (args.output / 'input.json').write_text(json.dumps([list(v) for v in values]))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  receipt = dict(cases=len(values), binding_sha256=provenance['module_sha256'],
                 source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', native_compared=False)
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps([list(v) for v in values]).encode())
    (args.output / 'actual.json').write_bytes(raw)
    receipt.update(compare(json.loads(raw), expected), native_compared=True)
  (args.output / 'receipt.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()

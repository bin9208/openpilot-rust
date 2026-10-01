#!/usr/bin/env python3
import argparse
import ast
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
import types
from typing import Optional

import capnp
from openpilot.cereal import log
from ublox_fixture import chunks


def source_parser():
  root = Path(__file__).resolve().parents[2]
  tree = ast.parse((root / 'openpilot/cereal/messaging/__init__.py').read_text())
  node = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'new_message')
  namespace = {'Optional': Optional, 'capnp': capnp, 'log': log, 'time': types.SimpleNamespace(monotonic=lambda: 1.23456789)}
  exec(compile(ast.Module(body=[node], type_ignores=[]), 'source-new-message', 'exec'), namespace)
  shim = types.ModuleType('openpilot.cereal.messaging')
  shim.new_message = namespace['new_message']
  sys.modules[shim.__name__] = shim
  return importlib.import_module('openpilot.system.ubloxd.ubloxd').UbloxMsgParser()


def decoded(packet):
  with log.Event.from_bytes(bytes(packet)) as event:
    return event.to_dict()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = chunks()
  (args.evidence / 'decoder-input.json').write_text(json.dumps(request))
  source = source_parser()
  expected = []
  for chunk in request:
    frames = source.framer.add_data(chunk['time'], bytes(chunk['bytes']))
    results = []
    for frame in frames:
      try:
        result = source.parse_frame(frame)
      except Exception:
        results.append({'error': True})
        continue
      if result is None:
        results.append({'skip': True})
      else:
        service, event = result
        results.append({'service': service, 'event': decoded(event.to_bytes())})
    expected.append(
      {'frames': [list(frame) for frame in frames], 'results': results, 'buffer': list(source.framer.buf), 'last_log_time': source.framer.last_log_time}
    )
  child = subprocess.run([args.trace.resolve()], input=json.dumps(request), text=True, capture_output=True, env=os.environ)
  (args.evidence / 'decoder-native.stderr').write_text(child.stderr or '(no stderr)\n')
  child.check_returncode()
  actual = json.loads(child.stdout)
  for chunk in actual:
    for result in chunk['results']:
      if 'packet' in result:
        result['event'] = decoded(result.pop('packet'))
  for name, result in [('source', expected), ('native', actual)]:
    (args.evidence / f'decoder-{name}.json').write_text(json.dumps(result, indent=2) + '\n')
  for i, (source_row, native_row) in enumerate(zip(expected, actual, strict=True)):
    assert source_row == native_row, f'decoder mismatch chunk {i}: {source_row["results"]} != {native_row["results"]}'
  packets = [row for chunk in actual for row in chunk['results'] if 'event' in row]
  result = {
    'pass_': True,
    'chunks': len(request),
    'frames': sum(len(row['frames']) for row in actual),
    'publications': len(packets),
    'services': sorted({row['service'] for row in packets}),
  }
  (args.evidence / 'decoder-results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

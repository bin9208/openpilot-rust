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


def source_trace(request):
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
  return expected


def corrected_oracle(request, original):
  # #317: independently reassemble only the known split NAV-PVT fixture input.
  # The original framer and decoder implementation remain unchanged.
  assert len(request) >= 2 and request[0]['time'] == request[1]['time']
  prefix, suffix = bytes(request[0]['bytes']), bytes(request[1]['bytes'])
  assert prefix == b'garbage\xb5' and suffix[:3] == b'\x62\x01\x07'
  assembled = source_trace([{'time': request[1]['time'], 'bytes': list(prefix + suffix)}, *request[2:]])
  old_frames = [frame for row in original for frame in row['frames']]
  new_frames = [frame for row in assembled for frame in row['frames']]
  old_results = [result for row in original for result in row['results']]
  new_results = [result for row in assembled for result in row['results']]
  assert len(old_frames) == 57 and len(new_frames) == 58
  assert new_frames[0] == list(prefix[-1:] + suffix)
  assert new_frames[1:] == old_frames, 'correction must preserve all 57 prior ordered frames'
  assert new_results[1:] == old_results, 'correction must preserve every prior decoded field'
  assert new_results[0]['service'] == 'gpsLocationExternal'
  assert len(old_results) == 57 and len(new_results) == 58
  assert sum('event' in result for result in old_results) == 17
  assert sum('event' in result for result in new_results) == 18
  assert assembled[1:] == original[2:], 'later source chunks must remain identical'
  assert original[0]['frames'] == original[0]['results'] == original[0]['buffer'] == []
  assert original[1]['frames'] == original[1]['results'] == original[1]['buffer'] == []
  # The first read contains no packet, only the independently identified sync byte.
  retained = {'frames': [], 'results': [], 'buffer': [0xb5], 'last_log_time': request[0]['time']}
  return [retained, *assembled]


def assert_equivalent(original, corrected, actual):
  old_frames = [frame for row in original for frame in row['frames']]
  new_frames = [frame for row in corrected for frame in row['frames']]
  old_results = [result for row in original for result in row['results']]
  new_results = [result for row in corrected for result in row['results']]
  assert len(old_frames) == 57 and len(new_frames) == 58
  assert new_frames[1:] == old_frames and new_results[1:] == old_results
  for i, (source_row, native_row) in enumerate(zip(corrected, actual, strict=True)):
    assert source_row == native_row, f'decoder mismatch chunk {i}: {source_row} != {native_row}'
  return corrected


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = chunks()
  (args.evidence / 'decoder-input.json').write_text(json.dumps(request))
  expected = source_trace(request)
  corrected = corrected_oracle(request, expected)
  child = subprocess.run([args.trace.resolve()], input=json.dumps(request), text=True, capture_output=True, env=os.environ)
  (args.evidence / 'decoder-native.stderr').write_text(child.stderr or '(no stderr)\n')
  child.check_returncode()
  actual = json.loads(child.stdout)
  for chunk in actual:
    for result in chunk['results']:
      if 'packet' in result:
        result['event'] = decoded(result.pop('packet'))
  for name, result in [('source', expected), ('corrected-source', corrected), ('native', actual)]:
    (args.evidence / f'decoder-{name}.json').write_text(json.dumps(result, indent=2) + '\n')
  assert_equivalent(expected, corrected, actual)
  packets = [row for chunk in actual for row in chunk['results'] if 'event' in row]
  result = {
    'pass_': True,
    'original_frames': 57,
    'original_publications': 17,
    'intentional_corrections': ['#317 retain split sync prefix: one independently decoded NAV-PVT'],
    'chunks': len(request),
    'frames': sum(len(row['frames']) for row in actual),
    'publications': len(packets),
    'services': sorted({row['service'] for row in packets}),
  }
  (args.evidence / 'decoder-results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

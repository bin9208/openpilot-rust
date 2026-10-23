#!/usr/bin/env python3
"""Hosted PSA asset/codec verification; this does not admit the unported vehicle runtime."""
from __future__ import annotations

import argparse
import ast
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess

from can_cases import codec
from can_source import ROOT, load, trace
from check_can import compare


def requirements() -> dict[str, set[str]]:
  required = {}
  path = ROOT / 'opendbc_repo/opendbc/car/psa/carstate.py'
  for node in ast.walk(ast.parse(path.read_text())):
    if (isinstance(node, ast.Subscript) and isinstance(node.slice, ast.Constant)
        and isinstance(node.slice.value, str) and isinstance(node.value, ast.Subscript)):
      message = node.value
      if (isinstance(message.value, ast.Attribute) and message.value.attr == 'vl'
          and isinstance(message.slice, ast.Constant) and isinstance(message.slice.value, str)):
        required.setdefault(message.slice.value, set()).add(node.slice.value)
  # Read the exact source command dictionary, not a separately maintained vehicle policy.
  path = ROOT / 'opendbc_repo/opendbc/car/psa/psacan.py'
  function = next(node for node in ast.parse(path.read_text()).body
                  if isinstance(node, ast.FunctionDef) and node.name == 'create_lka_steering')
  call = next(node for node in ast.walk(function) if isinstance(node, ast.Call)
              and isinstance(node.func, ast.Attribute) and node.func.attr == 'make_can_msg')
  values = next(node.value for node in function.body if isinstance(node, ast.Assign)
                and any(isinstance(target, ast.Name) and target.id == 'values' for target in node.targets))
  required[ast.literal_eval(call.args[0])] = {ast.literal_eval(key) for key in values.keys}
  assert len(required) == 13 and sum(map(len, required.values())) == 30
  return required


def cases(report: dict) -> list[dict]:
  path = ROOT / 'opendbc_repo/opendbc/dbc/psa_aee2010_r3.dbc'
  data = path.read_bytes()
  assert hashlib.sha1(f'blob {len(data)}\0'.encode() + data).hexdigest() == 'fb9a0c5159a7501a83a447234761ce58842f6eeb'
  DBC, CANPacker, _ = load()
  from opendbc.car.psa.psacan import create_lka_steering
  dbc = DBC(str(path))
  required = requirements()
  report['required_signals'] = {name: sorted(signals) for name, signals in required.items()}
  report['missing_signals'] = {name: sorted(signals - dbc.name_to_msg[name].sigs.keys()) if name in dbc.name_to_msg else sorted(signals)
                              for name, signals in required.items()}
  assert not any(report['missing_signals'].values()), report['missing_signals']
  packer = CANPacker(str(path))
  steps = []
  # Exercise the exact fork steering caller, including its checksum through the generic packer.
  for index, (active, angle, status) in enumerate(((False, -12.5, 0), (True, 0., 4), (True, 12.5, 4))):
    address, frame, bus = create_lka_steering(packer, active, angle, status)
    steps.append(dict(op='pack', address=address, values=[['DRIVE', 1], ['STATUS', status], ['LXA_ACTIVATION', 1],
                      ['TORQUE_FACTOR', 100 * active], ['SET_ANGLE', angle]], rx_counter=None))
    steps.append(dict(op='update', packets=[dict(mono_time=2_000_000_000 + index * 50_000_000,
                      frames=[dict(address=address, data=list(frame), bus=bus)])], checks=1))
  for index in range(16):
    frames = []
    for name in required:
      message = dbc.name_to_msg[name]
      values = {s.name: (index % (1 << min(s.size, 3))) * s.factor + s.offset for s in message.sigs.values()}
      address, frame, bus = packer.make_can_msg(name, 0, values)
      frames.append(dict(address=address, data=list(frame), bus=bus))
    steps.append(dict(op='update', packets=[dict(mono_time=2_200_000_000 + index * 50_000_000, frames=frames)], checks=1))
  report['asset_sha256'] = hashlib.sha256(data).hexdigest()
  return [codec(str(path)), dict(path=str(path), bus=0, now=2_000_000_000,
          messages=[[name, 20, False] for name in required], steps=steps)]


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=False)
  report = {'runtime_admitted': False}
  captured = io.StringIO()
  try:
    with contextlib.redirect_stdout(captured), contextlib.redirect_stderr(captured):
      request = cases(report)
      expected = [trace(case) for case in request]
    assert not [step for case in expected for step in case['steps'] if 'error' in step]
    # The maintained source packer returns empty bytes for the final unknown address.
    assert expected[0]['steps'][-1] == {'packed': []}
    for index in (0, 2, 4):
      # Require the codec request to match the actual source steering caller, counters included.
      assert expected[1]['steps'][index]['packed'] == request[1]['steps'][index + 1]['packets'][0]['frames'][0]['data']
    (args.evidence / 'input.json').write_text(json.dumps({'cases': request}) + '\n')
    (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
    target = args.evidence / 'native.json'
    child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps({'cases': request}), text=True, capture_output=True)
    (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
    child.check_returncode()
    compare(expected, json.loads(target.read_text()))
    report.update(status='pass', messages=len(expected[0]['dbc']['messages']), cases=len(request),
                  binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest())
  finally:
    (args.evidence / 'source.log').write_text(captured.getvalue())
    (args.evidence / 'partial.json').write_text(json.dumps(report, indent=2) + '\n')
  (args.evidence / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()

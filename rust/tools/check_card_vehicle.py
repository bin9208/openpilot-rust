import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import random
import subprocess
from card_vehicle_source import ROOT, decode, trace


def cases() -> list[dict]:
  catalog = json.loads((ROOT / 'rust/crates/card/data/vehicle.json').read_text())
  result = []
  for platform in catalog['platforms']:
    for options in (None, {'deadzone_deg': .1, 'use_steering_angle': False}):
      for nnff, disable_min, not_car, angle in ((False, False, False, False), (True, True, False, False), (True, False, True, True)):
        result.append(dict(op='params', candidate=platform['candidate'], torque=options, nnff=nnff, disable_min=disable_min,
                           not_car=not_car, angle=angle, firmware=[dict(ecu='eps', fw_version=list(b"'eps\\n\xff"))]))
  result.append(dict(op='params', candidate='unknown', torque=None, nnff=False, disable_min=False, not_car=False, angle=False, firmware=[]))
  rng = random.Random(177)
  steps = []
  for index in range(5000):
    value = (index % 400) / 20 + rng.random() / 10
    steps.append(dict(op='speed', value=value))
    steps.append(dict(op='lamp' if index % 3 else 'stalk', time=index % 20, left=bool(rng.getrandbits(1)), right=bool(rng.getrandbits(1))))
    steps.append(dict(op='pressed', pressed=bool(rng.getrandbits(1)), minimum=index % 10))
  for gear in (None, '', 'Park', 'P', 'R', 'reverse', 'N', 'neutral', 'ECO', 'T', 'MANUAL', 'Drive', 's', 'low', 'brake', 'bad', 'ｐ', 'ſ'):
    steps.append(dict(op='gear', value=gear))
  for pcm in (False, True):
    for kind in range(10):
      for pressed in (False, True):
        steps.append(dict(op='buttons', pcm=pcm, events=[(kind, pressed)]))
  for unit in (1 / 3.6, .44704, 1.):
    for factor in (.9, 1., 1.1):
      steps.append(dict(op='wheels', values=[1., 2., 3., 4.], unit=unit, factor=factor))
  result.append(dict(op='state', steps=steps))
  return result


def compare(left, right, path='root') -> None:
  if isinstance(left, dict) and isinstance(right, dict):
    assert left.keys() == right.keys(), (path, left.keys(), right.keys())
    for key in left:
      compare(left[key], right[key], f'{path}.{key}')
  elif isinstance(left, list) and isinstance(right, list):
    assert len(left) == len(right), (path, len(left), len(right))
    for index, (a, b) in enumerate(zip(left, right, strict=True)):
      compare(a, b, f'{path}[{index}]')
  else:
    assert left == right, (path, left, right)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = cases()
  source_log = io.StringIO()
  with redirect_stdout(source_log), redirect_stderr(source_log):
    expected = [trace(case) for case in request]
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'no source diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = [decode(value) for value in json.loads(target.read_text())]
  (args.evidence / 'native-decoded.json').write_text(json.dumps(actual) + '\n')
  try:
    compare(expected, actual)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), state_operations=len(request[-1]['steps']),
                source_failures=sum('error' in case for case in expected if isinstance(case, dict)),
                observable='complete decoded CarParams, NNFF setting writes, state helper transitions; scalar and Float32 wire values exact',
                runtime_python=False, binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                source_sha256=json.loads((ROOT / 'rust/crates/card/data/vehicle-provenance.json').read_text())['source_sha256'])
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

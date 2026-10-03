# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
from dataclasses import asdict
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import TypedDict, assert_never

from bluetooth_gesture_cases import Case, cases

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from openpilot.selfdrive.carrot.bluetooth.model import Decoder


class Step(TypedDict):
  tokens: list[str]
  active: list[str]
  repeated: list[str]


def original(case: Case) -> list[Step]:
  decoder = Decoder(case.profile, case.mapping, case.learning)
  result: list[Step] = []
  for operation in case.operations:
    match operation['op']:
      case 'feed':
        event = operation['event']
        tokens = decoder.feed(event['kind'], event['code'], event['value'], event['at'])
      case 'flush':
        tokens = decoder.flush(operation['at'])
      case 'cancel':
        decoder.cancel_holds()
        tokens = []
      case unreachable:
        assert_never(unreachable)
    result.append({'tokens': tokens, 'active': sorted(decoder.active_longs), 'repeated': sorted(decoder.repeated)})
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  inputs = cases(ROOT)
  payload = ''.join(json.dumps(asdict(case)) + '\n' for case in inputs)
  (args.output / 'input.jsonl').write_text(payload)
  completed = subprocess.run([str(args.binary.resolve())], input=payload, text=True, capture_output=True, timeout=60)
  (args.output / 'native.jsonl').write_text(completed.stdout)
  (args.output / 'native.stderr').write_text(completed.stderr)
  completed.check_returncode()
  expected = [original(case) for case in inputs]
  (args.output / 'original.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
  actual = [json.loads(line) for line in completed.stdout.splitlines()]
  assert len(actual) == len(expected)
  for number, (left, right) in enumerate(zip(expected, actual, strict=True)):
    assert len(left) == len(right), number
    for index, (source, native) in enumerate(zip(left, right, strict=True)):
      assert source == native, (number, index, inputs[number].operations[index], source, native)
  summary = {'cases': len(inputs), 'steps': sum(len(case.operations) for case in inputs),
             'tokens': sum(len(step['tokens']) for case in expected for step in case),
             'source_sha256': hashlib.sha256((ROOT / 'openpilot/selfdrive/carrot/bluetooth/model.py').read_bytes()).hexdigest(),
             'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(summary, indent=2))
  print(json.dumps(summary))


if __name__ == '__main__':
  main()

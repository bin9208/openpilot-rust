import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import unicodedata

from openpilot.selfdrive.carrot.bluetooth.bluez import Bluez


class Answer:
  def __init__(self):
    self.value = None
    self.settled = False

  def done(self) -> bool:
    return self.settled

  def set_result(self, value) -> None:
    self.value = value
    self.settled = True


def original(kind: str, value) -> dict:
  client = Bluez()
  client.prompt = {'id': 'fixture', 'kind': kind}
  client.answer = Answer()
  try:
    client.respond('fixture', value)
    assert client.answer.settled
    return {'accepted': True}
  except ValueError as error:
    return {'error': str(error)}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  kinds = ['RequestConfirmation', 'RequestAuthorization', 'AuthorizeService', 'RequestPinCode',
           'RequestPasskey', 'DisplayPinCode', 'DisplayPasskey']
  values = [None, False, True, -1, 0, 1, 999999, 1000000, 10**100, 1.0, -0.0, float('inf'), float('nan'),
            [], {}, ['1'], '', '0', '000001', '123456', '1234567', ' 1 ', '+1', '1_2', '1.0', '-1',
            '\0', '\ud800', '한글', '가' * 16, '가' * 17, '🙂' * 16, '🙂' * 17]
  digits = [[point, unicodedata.decimal(chr(point), None)] for point in range(0x110000) if chr(point).isdigit()]
  values.extend(chr(point) for point, _ in digits)
  values.extend(''.join(chr(point + offset) for offset in range(6)) for point, value in digits if value == 0)
  cases = [{'kind': kind, 'value': value} for kind in kinds for value in values]
  payload = ''.join(json.dumps(case) + '\n' for case in cases)
  (args.output / 'cases.jsonl').write_text(payload)
  expected = [original(case['kind'], case['value']) for case in cases]
  (args.output / 'source.json').write_text(json.dumps(expected))
  result = subprocess.run([str(args.binary.resolve())], input=payload, capture_output=True, text=True, timeout=30)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  result.check_returncode()
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert expected == actual, next((index, cases[index], left, right) for index, (left, right) in enumerate(zip(expected, actual, strict=True)) if left != right)
  result = subprocess.run([str(args.binary.resolve()), '--digits'], capture_output=True, text=True, check=True, timeout=30)
  assert json.loads(result.stdout) == digits
  receipt = {'cases': len(cases), 'accepted': sum(item.get('accepted', False) for item in expected),
             'unicode_points_checked': 0x110000, 'digit_points': len(digits), 'unicode_version': unicodedata.unidata_version,
             'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()

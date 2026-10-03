"""Capture exact original-source/native diagnostic transport differential evidence."""
import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess

from can_source import ROOT
from check_can import compare
from card_isotp_cases import cases
from card_isotp_source import run


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source_log = io.StringIO()
  request = cases()
  with redirect_stdout(source_log), redirect_stderr(source_log):
    expected = run(request)
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'source produced no diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  # Transport failure classes and payload/state/I/O are exact. Source error prose is retained separately.
  def compare_transport(left, right, location='root'):
    match left, right:
      case dict() as expected_map, dict() as actual_map:
        if 'error' in expected_map:
          assert expected_map['error'] == actual_map.get('error'), (location, expected_map, actual_map)
        else:
          assert expected_map.keys() == actual_map.keys(), (location, expected_map.keys(), actual_map.keys())
          for key in expected_map:
            compare_transport(expected_map[key], actual_map[key], f'{location}.{key}')
      case list() as expected_list, list() as actual_list:
        assert len(expected_list) == len(actual_list), (location, len(expected_list), len(actual_list))
        for index, (one, two) in enumerate(zip(expected_list, actual_list, strict=True)):
          compare_transport(one, two, f'{location}[{index}]')
      case _:
        compare(left, right, location)
  try:
    compare_transport(expected, actual)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request['cases']), operations=sum(len(case['steps']) for case in request['cases']),
                addresses=len(request['addresses']), runtime_python=False,
                source_sha256=hashlib.sha256((ROOT / 'opendbc_repo/opendbc/car/uds.py').read_bytes()).hexdigest(),
                observable='CAN address/bus/bytes, delay, receive-call count, timeout clock, all ISO-TP/client state and exception classes',
                scope='CanClient/IsoTpMessage transport only; active query orchestration and complete card interfaces/daemon pending')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

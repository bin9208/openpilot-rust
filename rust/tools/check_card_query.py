# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: python3 rust/tools/check_card_query.py --binary <query_trace> --evidence <directory>
"""Capture unchanged-source/native parallel query transport and timing parity."""
import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess
from typing import TypeAlias

from can_source import ROOT
from card_query_cases import cases
from card_query_source import trace
from check_can import compare

Json: TypeAlias = str | int | float | bool | None | list['Json'] | dict[str, 'Json']


def normalized(value: Json) -> Json:
  match value:
    case dict() as entries:
      return {('exception_class' if key == 'error' else key): normalized(item)
              for key, item in entries.items() if key != 'detail'}
    case list() as entries:
      return [normalized(item) for item in entries]
    case str() | int() | float() | bool() | None:
      return value
    case unreachable:
      from typing import assert_never
      assert_never(unreachable)


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
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'source produced no diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  try:
    compare(normalized(expected), normalized(actual))
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), runtime_python=False,
                source_sha256=hashlib.sha256((ROOT / 'opendbc_repo/opendbc/car/isotp_parallel_query.py').read_bytes()).hexdigest(),
                binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                observable='target/subaddress result order and payload, exact CAN address/bus/bytes, delays, receive waits, monotonic clock and exception classes',
                scope='Parallel diagnostic query orchestration; complete card interfaces and daemon remain pending')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

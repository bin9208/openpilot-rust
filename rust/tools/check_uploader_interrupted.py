#!/usr/bin/env python3
"""Retry interrupted upload_success sends without repeating HTTP or losing markers."""

import argparse
import json
import os
from pathlib import Path

from check_interrupted_send import trace_rows
from check_uploader_daemon import scenario


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  trace = output / 'send.tsv'
  os.environ.update(ZMQ_TEST_SEND_MATCH='upload_success', ZMQ_TEST_SEND_ERRORS='4,4,4,0', ZMQ_TEST_SEND_TRACE=str(trace))
  result = scenario(args.binary.resolve(), output, 'force-none-metered', {'force': True, 'network': 'none', 'metered': True})
  rows = trace_rows(trace)
  assert [row['errno'] for row in rows] == [4, 4, 4, 0, 0], rows
  assert [row['injected'] for row in rows] == [1, 1, 1, 0, 0]
  assert len({row['packet'] for row in rows[:4]}) == 1
  assert all(row['flags'] == 1 for row in rows)
  accepted = [bytes.fromhex(row['packet']) for row in rows if row['result'] >= 0]
  actual = [path.read_bytes() for path in (output / 'force-none-metered').glob('log-*.packet')]
  assert all(actual.count(packet) == 1 for packet in accepted)
  result.update(interrupted_attempts=3, accepted_success_records=2, qlog_and_qcamera_marked=True)
  (output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

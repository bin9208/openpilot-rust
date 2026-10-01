#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess

import capnp
from openpilot.cereal import log
from locationd_fixture import camera
from locationd_source import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source, _ = load(args.oracle.resolve())
  estimator = source['LocationEstimator'](True)
  packet = camera(0.0)
  with log.Event.from_bytes(bytes(packet)) as event:
    estimator.handle_log(0.0, event.which(), getattr(event, event.which()))
  try:
    estimator.get_msg(True, True, True)
  except capnp.KjException as error:
    assert 'out-of-range' in str(error)
    source_error = str(error).split('stack:')[0]
  else:
    raise AssertionError('source accepted a negative UInt64 timestamp')
  child = subprocess.run(
    [args.trace.resolve(), args.evidence / 'native.json'],
    input=json.dumps({'cases': [{'name': 'negative-filter-time', 'events': [packet]}]}),
    text=True,
    capture_output=True,
  )
  result = {'source_error': source_error, 'native_exit': child.returncode, 'native_stderr': child.stderr}
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  assert child.returncode != 0, 'native silently saturated the negative filter timestamp'
  assert 'filter timestamp' in child.stderr
  print('PASS source and native reject negative filter publication timestamp')


if __name__ == '__main__':
  main()

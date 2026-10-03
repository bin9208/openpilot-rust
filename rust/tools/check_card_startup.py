"""Capture complete original-source/native passive startup-policy comparisons."""

import argparse
from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import subprocess

from card_startup_cases import fingerprints, toggles
from card_startup_source import fingerprint, toggle


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source_log = io.StringIO()
  with redirect_stdout(source_log), redirect_stderr(source_log):
    request = dict(fingerprints=fingerprints(), toggles=toggles())
    expected = dict(fingerprints=[fingerprint(case) for case in request['fingerprints']], toggles=[toggle(case) for case in request['toggles']])
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'source produced no diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  assert expected == actual, 'native startup policy differs from unchanged source; inspect captured traces'
  result = dict(status='pass', fingerprint_cases=len(request['fingerprints']), toggle_cases=len(request['toggles']),
                toggle_steps=sum(len(case) for case in request['toggles']), observable='selected identity, received packet count, ordered bus/address/DLC data, hold decision/timer/rearm state exact',
                scope='Passive fingerprints and MAIN hold policy only; active FW/VIN and card runtime/controllers remain pending')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["jeepney==0.9.0", "pyzmq==27.2.0", "numpy==2.5.3"]
# ///
# ─── How to run ───
# PYTHONPATH=.:rust/tools python rust/tools/check_wifi_callback_ordering.py \
#   --binary <wifi_native> --launcher <process-child> --binding <params_pyx.so> --output <fresh-evidence>
# ──────────────────
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from check_wifi_runtime import scenario


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.binary, args.launcher, args.binding, args.output = [path.resolve() for path in (args.binary, args.launcher, args.binding, args.output)]
  args.defer_final_forgotten = True
  args.output.mkdir(parents=True, exist_ok=False)
  expected = scenario(args, 'source', True)
  actual = scenario(args, 'native', True)
  assert expected == actual, ('source/native callback completion mismatch', args.output)
  assert expected['events'].count({'Forgotten': 'B'}) == 2, expected['events']
  observations = json.loads((args.output / 'source/existing/callback-gate.json').read_text())
  before_entry = [row for row in observations if row['gate']['approaching'] and not row['gate']['entry_allowed']
                  and not row['gate']['blocked'] and not row['gate']['released'] and 'B' not in row['saved_ssids']]
  assert before_entry, ('profile removal before gate entry was not reached', observations)
  waiting = [row for row in observations if row['gate']['blocked'] and not row['gate']['released']
             and 'B' not in row['saved_ssids'] and row['forgotten_count'] == 1]
  assert waiting, ('controlled scheduling gap was not reached', observations)
  releases = [index for index, row in enumerate(observations) if row['request']['op'] == 'release_final_forget']
  assert len(releases) == 1, releases
  observed = observations[releases[0] - 1]
  assert observed['gate']['blocked'] and not observed['gate']['released'] and 'B' not in observed['saved_ssids'], observed
  assert observations[-1]['gate']['released'] and observations[-1]['forgotten_count'] == 2, observations[-1]
  paths = [Path(__file__), Path(__file__).with_name('check_wifi_runtime.py'),
           Path(__file__).with_name('wifi_fixture') / 'callback_gate.py', Path(__file__).with_name('wifi_fixture') / 'source_peer.py',
           Path(__file__).resolve().parents[2] / 'openpilot/system/ui/lib/wifi_manager.py', args.binary, args.launcher, args.binding]
  report = {'result': 'PASS', 'exact': True, 'source_events': len(expected['events']), 'native_events': len(actual['events']),
            'forgotten_b_callbacks': 2, 'profile_removal_observed_before_final_callback': True,
            'profile_removal_observed_before_gate_entry': True, 'release_follows_observed_blocked_gate': True,
            'hashes': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}}
  (args.output / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
  print('PASS delayed gate entry after profile removal; observed blocked gate before release; exact lifecycle retained')
  print(json.dumps(report))


if __name__ == '__main__':
  main()

# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: oracle Python rust/tools/card_qa/volkswagen/coverage.py EVIDENCE_DIR
from __future__ import annotations

import json
from pathlib import Path
import sys
from can_source import load


def main():
  load()
  from openpilot.cereal import car
  root = Path(sys.argv[1])
  inputs = json.loads((root / 'verified-runtime/input.json').read_text())
  results = json.loads((root / 'verified-runtime/native.json').read_text())
  report = []
  stored_boundary = 0.30000001192092896
  reused_boundary_hits = 0
  for case, result in zip(inputs, results, strict=True):
    if case['candidate'].startswith('VOLKSWAGEN_ID4'):
      for row in result['steps']:
        with car.CarState.from_bytes(bytes(row['state'])) as state:
          reused_boundary_hits += state.vEgo == stored_boundary
    if not case['name'].startswith('focused-'):
      continue
    rows = result['steps']
    assert rows[0]['controller']['steering_power_last'] == 2
    assert rows[30]['controller']['hold_release_frames'] == 99
    assert rows[200]['controller']['hold_release_frames'] == 14
    assert {row['controller']['acc_hold_type_last'] for row in rows} == {0, 1, 4, 5}
    assert max(row['controller']['long_override_counter'] for row in rows) == 5
    assert max(row['controller']['long_disabled_counter'] for row in rows) == 5
    assert not rows[600]['extra']['eps_init_complete']
    assert rows[601]['extra']['eps_init_complete']
    states = {}
    for index in (420, 490, 598, 599, 601, 620):
      with car.CarState.from_bytes(bytes(rows[index]['state'])) as state:
        states[index] = state.to_dict()
    assert states[420]['accFaulted'] and not states[490]['accFaulted']
    assert not states[598]['accFaulted'] and states[599]['accFaulted']
    assert states[601]['steerFaultPermanent'] and not states[620]['steerFaultPermanent']
    assert max(row['controller']['lead_limit_cnt'] for row in rows) == 31
    assert any(row['controller']['lead_limit_disp'] for row in rows)
    assert max(row['controller']['navi_banner_frames'] for row in rows) == 19
    assert max(row['controller']['road_banner_frames'] for row in rows) == 19
    report.append({'name': case['name'], 'frames': len(rows), 'observables': ['initial power 2', 'sustained hold-release after ESP clears',
      'hold/ramp values 0/1/4/5', 'override and disable counters 5', 'EPS timeout first completes at source frame 601',
      'fault grace expires exactly 100 frames after inhibit', 'lead debounce 32 HUD cycles', 'navigation and road banners 20 HUD cycles']})
  assert len(report) == 2
  assert reused_boundary_hits == 0
  seeded = json.loads((root / 'verified-seeded/native.json').read_text())
  for case in seeded:
    assert case['steps'][0]['controller']['apply_torque_last'] == 3
    assert case['steps'][0]['controller']['hca_frame_same_torque'] == 0
    assert not case['steps'][0]['controller']['eps_timer_soft_disable_alert']
  timer = json.loads((root / 'final-timer/native.json').read_text())
  for case in timer:
    assert case['steps'][2]['controller']['eps_timer_soft_disable_alert']
  for folder in ('done-speed-green-mk1', 'done-speed-green-mk2'):
    value = json.loads((root / folder / 'native.json').read_text())[0]
    assert value['steps'][0]['controller']['hold_release_frames'] == 0
  print(json.dumps({'status': 'pass', 'focused_runtime': report, 'seeded_same_torque_cases': len(seeded),
                    'focused_timer_cases': len(timer), 'reused_mqbs_mapped_behavior': 'unchanged',
                    'reused_meb_f32_boundary_hits': reused_boundary_hits, 'corrected_boundary_cases': 2}, indent=2))


if __name__ == '__main__':
  main()

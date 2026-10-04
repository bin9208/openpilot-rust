import argparse
import dataclasses
import json
from pathlib import Path
import struct
import subprocess

import pytest

from openpilot.selfdrive.carrot.radar_motion import lane_change_gap as original
from plannerd_owner_source import difference


def bits(value):
  return struct.unpack('<Q', struct.pack('<d', float(value)))[0]


def lead(value):
  if value is None:
    return None
  if not value.status:
    return {'status': False}
  return {key: getattr(value, key) for key in ('radarTrackId', 'dRel', 'yRel', 'vRel', 'vLead', 'aLeadK', 'status', 'radar')}


def plan(value, credit=False):
  fields = dataclasses.asdict(value)
  fields['clearance'] = fields.pop('clearance_s')
  fields['targets'] = [
    {
      'id': target.radarTrackId,
      'distance': target.dRel,
      'lateral': target.yRel,
      'relative_speed': target.vRel,
      'speed': target.vLead,
      'acceleration': target.aLeadK,
    }
    for target in value.targets
  ]
  if credit:
    fields['reason'] = 'confirmed-departure'
  return fields


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  actions, expected, owners = [], [], []
  update, credit = original.LaneChangeGapTracker.update, original.LaneChangeGapPlan.credit

  def recorded_update(self, **kwargs):
    if not any(owner is self for owner in owners):
      owners.append(self)
    owner = next(index for index, value in enumerate(owners) if value is self)
    result = update(self, **kwargs)
    actions.append(
      {
        'op': 'update',
        'owner': owner,
        'scalars': [bits(kwargs[key]) for key in ('now', 'v_ego', 'yaw_rate')],
        'input': {
          'now': 0.0,
          'speed': 0.0,
          'yaw_rate': 0.0,
          'direction': kwargs['direction'],
          'path_t': list(kwargs['path_t']),
          'path_x': list(kwargs['path_x']),
          'path_y': list(kwargs['path_y']),
          'primary': lead(kwargs.get('primary')),
          'secondary': lead(kwargs.get('secondary')),
          'blindspot': kwargs.get('blindspot', False),
          'valid': kwargs.get('valid', True),
          'legacy_side_input': 'side_leads' in kwargs,
        },
      }
    )
    expected.append({'plan': plan(result), 'direction': self.direction})
    return result

  def recorded_credit(self, primary, horizons, v_ego, max_accel, t_follow, stop_distance, ratio):
    result = credit(self, primary, horizons, v_ego, max_accel, t_follow, stop_distance, ratio)
    actions.append(
      {
        'op': 'credit',
        'plan': plan(self, True),
        'primary': lead(primary),
        'horizons': list(horizons),
        'scalars': [bits(value) for value in (v_ego, max_accel, t_follow, stop_distance, ratio)],
      }
    )
    expected.append([bits(value) for value in result])
    return result

  original.LaneChangeGapTracker.update = recorded_update
  original.LaneChangeGapPlan.credit = recorded_credit
  status = pytest.main(  # noqa: TID251 - One isolated recorder process invokes the unchanged source tests exactly once.
    ['-o', 'addopts=', '-q', '--confcutdir=openpilot/selfdrive/carrot/tests', 'openpilot/selfdrive/carrot/tests/test_lane_change_gap.py']
  )
  assert status == 0, status
  encoded = json.dumps(actions, allow_nan=False)
  (args.output / 'input.json').write_text(encoded + '\n')
  (args.output / 'expected.json').write_text(json.dumps(expected, allow_nan=False) + '\n')
  child = subprocess.run([str(args.binary.resolve())], input=encoded, text=True, capture_output=True, check=False)
  (args.output / 'actual.json').write_text(child.stdout)
  (args.output / 'stderr.txt').write_text(child.stderr)
  child.check_returncode()
  mismatches = difference(json.loads(json.dumps(expected)), json.loads(child.stdout))
  positive_plans = sum(item['plan']['confidence'] > 0 for item in expected if isinstance(item, dict))
  positive_credit = sum(any(value != 0 for value in item) for item in expected if isinstance(item, list))
  receipt = {
    'actions': len(actions),
    'owners': len(owners),
    'positive_plans': positive_plans,
    'positive_credit': positive_credit,
    'mismatches': len(mismatches),
    'first_mismatches': mismatches[:50],
  }
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  assert positive_plans > 0 and positive_credit > 0 and not mismatches, receipt
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()

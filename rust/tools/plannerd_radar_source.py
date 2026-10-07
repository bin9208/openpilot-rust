"""Record unchanged source pytest scenarios and compare Rust planner radar policies."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys

FLOATS = (
  'dRel',
  'yRel',
  'vRel',
  'aRel',
  'vLead',
  'dPath',
  'vLat',
  'vLeadK',
  'aLeadK',
  'aLeadTau',
  'modelProb',
  'aLead',
  'jLead',
  'score',
  'cutOutTime',
  'cutOutConfidence',
)


def bits(value):
  return struct.unpack('<Q', struct.pack('<d', float(value)))[0]


def radar(value):
  return [
    {
      'floats': [bits(getattr(lead, key)) for key in FLOATS],
      'status': bool(lead.status),
      'radar': bool(lead.radar),
      'id': int(lead.radarTrackId),
      'fcw': bool(lead.fcw),
    }
    for lead in (value.leadOne, value.leadTwo, value.leadCutInRisk)
  ]


class Recorder:
  def __init__(self, output):
    self.output = output
    self.actions, self.expected = [], []
    self.instances = 0

  def record(self, action, expected=None, **fields):
    self.actions.append({'action': action, **fields})
    self.expected.append(expected)

  def save(self, value):
    builder = value.as_builder() if hasattr(value, 'as_builder') else value.as_reader().as_builder()
    data = builder.to_bytes()
    path = self.output / (hashlib.sha256(data).hexdigest() + '.bin')
    if not path.exists():
      path.write_bytes(data)
    return str(path)

  def instrument(self):
    from openpilot.selfdrive.controls.lib.longitudinal_fast_radar import FastRadarOverlay
    from openpilot.selfdrive.controls.lib.longitudinal_stopping_lead import StoppingLeadFilter

    fast_init, observe, ready, build = (
      FastRadarOverlay.__init__,
      FastRadarOverlay.observe_radar_state,
      FastRadarOverlay.lead_one_ready,
      FastRadarOverlay.build,
    )
    stop_init, stop = StoppingLeadFilter.__init__, StoppingLeadFilter.update
    owner = self

    def init_fast(instance, front_radar_delay_s):
      fast_init(instance, front_radar_delay_s)
      owner.instances += 1
      instance.trace_id = owner.instances
      owner.record('fast_new', id=instance.trace_id, delay=bits(front_radar_delay_s))

    def observe_fast(instance, state, time, valid):
      path = owner.save(state)
      result = observe(instance, state, time, valid)
      owner.record('observe', id=instance.trace_id, radar=path, time=int(time), valid=bool(valid))
      return result

    def ready_fast(instance, state):
      path = owner.save(state)
      result = ready(instance, state)
      owner.record('ready', bool(result), id=instance.trace_id, radar=path)
      return result

    def build_fast(instance, state, tracks, **kwargs):
      paths = owner.save(state), owner.save(tracks)
      result = build(instance, state, tracks, **kwargs)
      owner.record(
        'build',
        {
          'radar': radar(result.radar_state),
          'mask': result.lead_mask,
          'id': result.lead_one_track_id,
          'age': bits(result.selection_age_s),
          'reason': result.lead_one_reason,
        },
        id=instance.trace_id,
        radar=paths[0],
        points=paths[1],
        speed=bits(kwargs['v_ego']),
        radar_ns=int(kwargs['radar_state_mono_ns']),
        live_ns=int(kwargs['live_tracks_mono_ns']),
        radar_valid=bool(kwargs['radar_state_valid']),
        live_valid=bool(kwargs['live_tracks_valid']),
      )
      return result

    def init_stop(instance):
      stop_init(instance)
      owner.instances += 1
      instance.trace_id = owner.instances
      owner.record('stop_new', id=instance.trace_id)

    def update_stop(instance, state, **kwargs):
      path = owner.save(state)
      result = stop(instance, state, **kwargs)
      owner.record(
        'stop',
        {'radar': radar(result), 'mask': instance.held_mask},
        id=instance.trace_id,
        radar=path,
        stopping=bool(kwargs['stopping']),
        speed=bits(kwargs['v_ego']),
        time=int(kwargs['mono_time_ns']),
        valid=bool(kwargs.get('valid', True)),
      )
      return result

    FastRadarOverlay.__init__ = init_fast
    FastRadarOverlay.observe_radar_state = observe_fast
    FastRadarOverlay.lead_one_ready = ready_fast
    FastRadarOverlay.build = build_fast
    StoppingLeadFilter.__init__ = init_stop
    StoppingLeadFilter.update = update_stop


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  recorder = Recorder(args.output)
  recorder.instrument()
  import pytest

  tests = [Path('openpilot/selfdrive/controls/tests') / name for name in ('test_longitudinal_fast_radar.py', 'test_longitudinal_stopping_lead.py')]
  status = pytest.main(  # noqa: TID251 - One isolated recorder process invokes the unchanged source tests exactly once.
    ['-q', '-o', 'addopts=', '--confcutdir=openpilot/selfdrive/controls/tests', *map(str, tests)]
  )
  if status:
    raise RuntimeError(f'original source tests failed: {status}')
  data = json.dumps(recorder.actions, allow_nan=False).encode()
  (args.output / 'input.json').write_bytes(data)
  (args.output / 'expected.json').write_text(json.dumps(recorder.expected, allow_nan=False) + '\n')
  result = subprocess.run([str(args.binary.resolve())], input=data, capture_output=True, check=False)
  (args.output / 'actual.json').write_bytes(result.stdout)
  (args.output / 'stderr.txt').write_bytes(result.stderr)
  result.check_returncode()
  actual = json.loads(result.stdout)
  assert len(actual) == len(recorder.expected), (len(actual), len(recorder.expected))
  mismatches = [
    {'index': i, 'action': recorder.actions[i], 'expected': expected, 'actual': got}
    for i, (got, expected) in enumerate(zip(actual, recorder.expected, strict=True))
    if got != expected
  ]
  receipt = {
    'actions': len(actual),
    'instances': recorder.instances,
    'mismatches': mismatches,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'tests': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in tests},
  }
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  assert not mismatches, f'{len(mismatches)} policy mismatches; inspect receipt.json'
  print(json.dumps({'actions': len(actual), 'instances': recorder.instances, 'exact': True}))


if __name__ == '__main__':
  sys.exit(main())

"""Record final upstream controller regressions and compare native full state."""

import argparse
import ast
import json
from pathlib import Path
import runpy
import sys
from radar_final_source import load, source


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  load()
  from openpilot.selfdrive.carrot.radar_motion.stationary_lane import StoppingLaneCornerTracker
  from openpilot.selfdrive.carrot.radar_motion.in_lane import InLaneCornerTracker
  from radard_source_values import model, plain, number

  owners = []
  tracker_actions = []
  tracker_expected = []

  def record(kind, cls):
    original = cls.update

    def update(self, time, ego, md, points, vision, **kw):
      if not any(owner is self for owner in owners):
        owners.append(self)
      index = next(i for i, owner in enumerate(owners) if owner is self)
      result = original(self, time, ego, md, points, vision, **kw)
      tracker_actions.append(
        {
          'owner': index,
          'kind': kind,
          'time': number(time),
          'ego': number(ego),
          'model': model(md),
          'points': plain(points),
          'vision': plain(vision),
          'measurement': number(kw['measurement_time_s']) if kw.get('measurement_time_s') is not None else None,
        }
      )
      tracker_expected.append({'output': None if result is None else [plain(result[0]), result[1]], 'state': plain(self)})
      return result

    cls.update = update
    original_reset = cls.reset

    def reset(self):
      if not any(owner is self for owner in owners):
        owners.append(self)
      index = next(i for i, owner in enumerate(owners) if owner is self)
      original_reset(self)
      tracker_actions.append({'owner': index, 'kind': kind, 'reset': True})
      tracker_expected.append(None)

    cls.reset = reset

  record('stopping', StoppingLaneCornerTracker)
  record('in_lane', InLaneCornerTracker)
  import openpilot.selfdrive.carrot.radar.tools.radar_validation_replay as replay

  tree = ast.parse(source('openpilot/selfdrive/carrot/radar/tools/radar_validation_replay.py'))
  nodes = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == '_controller_model']
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'e8e152db:replay_model', 'exec'), replay.__dict__)
  tests = args.output.resolve() / 'source/openpilot/selfdrive/carrot/tests'
  tests.mkdir(parents=True, exist_ok=False)
  names = [
    'test_stopping_lane_corner.py',
    'test_in_lane_corner.py',
    'test_radar_forced_scc_gate.py',
    'test_radar_motion_predictor.py',
    'test_radar_corroboration.py',
    'test_trajectory_cutin.py',
    'test_trajectory_cutout.py',
  ]
  for _relative, original in [
    ('radar/radard_dpath.py', 'openpilot/selfdrive/carrot/radar/radard_dpath.py'),
    ('radar/tools/radar_validation_replay.py', 'openpilot/selfdrive/carrot/radar/tools/radar_validation_replay.py'),
    ('../../system/manager/process_config.py', 'openpilot/system/manager/process_config.py'),
  ]:
    destination = args.output.resolve() / 'source' / original
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(source(original))
  fixture = tests / 'fixtures/cutout_departure.json'
  fixture.parent.mkdir(parents=True, exist_ok=True)
  fixture.write_text(source('openpilot/selfdrive/carrot/tests/fixtures/cutout_departure.json'))
  for name in names:
    (tests / name).write_text(source('openpilot/selfdrive/carrot/tests/' + name))
  sys.argv = ['radard_controller_source.py', '--output', str(args.output.resolve() / 'comparison')]
  if args.binary is not None:
    sys.argv += ['--binary', str(args.binary.resolve())]
  for name in names:
    sys.argv += ['--test', str(tests / name)]
  # The existing recorder retains exact state/cache/float comparisons. Its
  # source metadata is replaced below with the actual verified source pin.
  runpy.run_path(str(Path(__file__).with_name('radard_controller_source.py')), run_name='__main__')
  (args.output / 'tracker-input.json').write_text(json.dumps(tracker_actions, allow_nan=False))
  (args.output / 'tracker-expected.json').write_text(json.dumps(tracker_expected, allow_nan=False))
  receipt = args.output.resolve() / 'comparison/receipt.json'
  value = json.loads(receipt.read_text())
  value['source_commit'] = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
  value['source_fixture'] = 'rust/upstream/radar-final-source.json'
  fixture_records = json.loads(Path(__file__).resolve().parents[1].joinpath('upstream/radar-final-source.json').read_text())['files']
  value['sources'] = {p: r['sha256'] for p, r in fixture_records.items()}
  value['tracker_steps'] = len(tracker_actions)
  receipt.write_text(json.dumps(value, indent=2) + '\n')


if __name__ == '__main__':
  main()

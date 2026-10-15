"""Final planner/MPC owners with inactive signal decisions (producer separate)."""

import argparse
import hashlib
import importlib
import json
import runpy
import sys
import types
from pathlib import Path
from planner_source import source


def register(path, name):
  value = types.ModuleType(name)
  value.__file__ = 'e8e152db:' + path
  sys.modules[name] = value
  exec(compile(source(path), value.__file__, 'exec'), value.__dict__)
  return value


def main():
  parser = argparse.ArgumentParser(add_help=False)
  parser.add_argument('--comfort', action='store_true')
  options, remaining = parser.parse_known_args()
  sys.argv = [sys.argv[0], *remaining]
  if options.comfort:
    import plannerd_owner_fixtures as fixtures

    original_messages = fixtures.messages
    original_parameters = fixtures.parameters

    def parameters():
      values = original_parameters()
      values['LeadAccelResponse'] = '5'
      return values

    def messages(factory, index, now):
      result = original_messages(factory, index, now)
      car = result['carState'].carState
      car.vEgo = car.vEgoCluster = 20.0
      car.aEgo = 0.0
      car.standstill = False
      car.gasPressed = car.brakePressed = car.leftBlinker = False
      car.softHoldActive = 0
      controls = result['controlsState'].controlsState
      controls.longControlState = 'pid'
      controls.forceDecel = False
      controls.curvature = controls.desiredCurvature = 0.0
      state = result['selfdriveState'].selfdriveState
      state.enabled = True
      state.experimentalMode = False
      fixtures.model(result['modelV2'].modelV2, 1000, 20.0)
      result['modelV2'].modelV2.meta.laneChangeState = 'off'
      result['modelV2'].modelV2.meta.desire = 'none'
      radar = result['radarState'].radarState
      radar.leadTwo.status = radar.leadCutInRisk.status = False
      lead = radar.leadOne
      lead.status = lead.radar = True
      lead.radarTrackId = 35
      lead.dRel = 45.0 if '--reserve-probe' in remaining else 60.0
      lead.yRel = 0.0
      lead.vRel = -1.0
      lead.vLead = lead.vLeadK = 19.0
      lead.aLead = lead.aLeadK = -6.0 if '--reserve-probe' in remaining else -1.0
      if '--reserve-probe' in remaining:
        lead.aLeadTau = 0.4
      result['carrotMan'].carrotMan.vTurnSpeed = 0
      return result

    fixtures.parameters = parameters
    fixtures.messages = messages
  original_import = importlib.import_module
  installed = False
  diagnostics = {"updates": 0, "solves": 0, "retry_frames": 0, "comfort_frames": 0, "lateral_run_solves": 0, "lateral_reset_solves": 0}

  def load(name, package=None):
    nonlocal installed
    if name == 'openpilot.selfdrive.controls.lib.longitudinal_planner' and not installed:
      installed = True
      for path in [
        'openpilot/common/stopping_params.py',
        'openpilot/selfdrive/carrot/signal_stop_prepare.py',
        *[
          'openpilot/selfdrive/controls/lib/' + part + '.py'
          for part in [
            'cruise_coasting',
            'lane_model_speed',
            'longitudinal_gap_recovery',
            'longitudinal_braking_gap',
            'longitudinal_mpc_lib/long_mpc',
            'lateral_mpc_lib/lat_mpc',
            'lateral_planner',
            'longitudinal_planner',
          ]
        ],
      ]:
        register(path, path.removesuffix('.py').replace('/', '.'))
      mpc = sys.modules['openpilot.selfdrive.controls.lib.longitudinal_mpc_lib.long_mpc'].LongitudinalMpc
      original_run, original_update = mpc.run, mpc.update

      def run(self, *args, **kwargs):
        diagnostics['solves'] += 1
        return original_run(self, *args, **kwargs)

      def update(self, *args, **kwargs):
        before = diagnostics['solves']
        result = original_update(self, *args, **kwargs)
        diagnostics['updates'] += 1
        diagnostics['retry_frames'] += diagnostics['solves'] - before > 1
        diagnostics['comfort_frames'] += any(s.relief > 0.0 for s in self.lead_comfort_states)
        return result

      mpc.run, mpc.update = run, update
      lateral = sys.modules['openpilot.selfdrive.controls.lib.lateral_mpc_lib.lat_mpc'].LateralMpc
      lateral_run, lateral_reset = lateral.run, lateral.reset

      def run_lateral(self, *args, **kwargs):
        diagnostics['lateral_run_solves'] += 1
        return lateral_run(self, *args, **kwargs)

      def reset_lateral(self, *args, **kwargs):
        diagnostics['lateral_reset_solves'] += 1
        return lateral_reset(self, *args, **kwargs)

      lateral.run, lateral.reset = run_lateral, reset_lateral
    module = original_import(name, package)
    if name == 'openpilot.selfdrive.carrot.carrot_functions':
      original = module.CarrotPlanner

      class Carrot(original):
        def __init__(self, *args, **kwargs):
          super().__init__(*args, **kwargs)
          self.prepare_decision = None
          self.prepare_control_active = False
          self.start_decision = None
          self.signal_detection_suspended = False
          self.signal_observation_color = 0
          self.signal_green_active = False

      module.CarrotPlanner = Carrot
    return module

  importlib.import_module = load
  try:
    runpy.run_path(str(Path(__file__).with_name('plannerd_owner_source.py')), run_name='__main__')
  finally:
    importlib.import_module = original_import
    if '--output' in remaining:
      output = Path(remaining[remaining.index('--output') + 1])
      if output.exists():
        receipt = output / 'receipt.json'
        if receipt.exists():
          value = json.loads(receipt.read_text())
          fixture = json.loads(Path(__file__).resolve().parents[1].joinpath('upstream/planner-source.json').read_text())
          value['source_commit'] = fixture['source_commit']
          value['source_files'] = {p: r['sha256'] for p, r in fixture['files'].items()}
          producer = Path('openpilot/selfdrive/carrot/carrot_functions.py')
          value['retained_carrot_producer_sha256'] = hashlib.sha256(producer.read_bytes()).hexdigest()
          value['inactive_signal_producer'] = True
          receipt.write_text(json.dumps(value, indent=2) + '\n')
        (output / 'final-source-scope.json').write_text(
          json.dumps({'source_commit': 'e8e152dbcfd9574a1791ed12026d1880e87689b3', 'inactive_signal_producer': True, **diagnostics}, indent=2) + '\n'
        )


if __name__ == '__main__':
  main()

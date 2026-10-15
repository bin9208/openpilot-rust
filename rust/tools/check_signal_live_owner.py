"""Final original MPC/Carrot owner and wire replay of connected native captures.

Reader/geometry context is recomputed by unchanged original read methods using
captured I/O clocks. Solver diagnostic durations are explicit captured metadata;
source execution duration is not compared with emulation or native duration.
"""
import argparse
import dataclasses
import importlib
import json
from pathlib import Path
import sys
from types import SimpleNamespace as NS
from openpilot.cereal import car, log
import check_signal_live_acquisition as acquisition
from check_planner_owner_final import register
from check_planner_signal_owner_final import register as register_signal, sanitize
from plannerd_owner_loader import SourceOwners
from plannerd_owner_snapshot import frame as snapshot
from plannerd_owner_source import difference
from signal_source import compare


def owners(native, values):
  original_import = importlib.import_module
  installed = False
  def load(name, package=None):
    nonlocal installed
    if name == 'openpilot.selfdrive.controls.lib.longitudinal_planner' and not installed:
      installed = True
      for path in ['openpilot/common/stopping_params.py', 'openpilot/selfdrive/carrot/signal_stop_prepare.py', *[
        'openpilot/selfdrive/controls/lib/' + part + '.py' for part in ['cruise_coasting', 'lane_model_speed', 'longitudinal_gap_recovery', 'longitudinal_braking_gap', 'longitudinal_mpc_lib/long_mpc', 'lateral_mpc_lib/lat_mpc', 'lateral_planner', 'longitudinal_planner']]]:
        register(path, path.removesuffix('.py').replace('/', '.'))
    return original_import(name, package)
  importlib.import_module = load
  try:
    owner = SourceOwners(native, values)
  finally:
    importlib.import_module = original_import
  for part in ['signal_assist', 'signal_mode', 'signal_assist_runtime', 'signal_start_guard', 'signal_stop_prepare_runtime', 'signal_road_policy', 'carrot_functions']:
    module = register_signal('openpilot/selfdrive/carrot/' + part + '.py')
    if part == 'signal_road_policy':
      module.time = NS(monotonic=lambda: owner.now)
    if part == 'carrot_functions':
      owner.carrot = module
  owner.long.time = NS(monotonic=lambda: owner.now)
  return owner


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source-native', type=Path, required=True)
  parser.add_argument('--input', type=Path, required=True)
  parser.add_argument('--capture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--source-only', action='store_true')
  parser.add_argument('--record-events', action='store_true', help='Require captured source record clocks and compare original record methods')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  request = json.loads(args.input.read_text())
  values = dict(request['parameters'], SignalAssistMode='2')
  owner = owners(args.source_native.resolve(), values)
  with car.CarParams.from_bytes(Path(request['car_params']).read_bytes()) as cp:
    long = owner.long.LongitudinalPlanner(cp)
    lat = owner.lat.LateralPlanner(cp, debug=False)
  from openpilot.selfdrive.carrot import signal_road_policy
  actual_carrot = owner.carrot.CarrotPlanner
  carrot = None
  contexts = {}
  wall_calls = iter(())
  owner.carrot.time = NS(monotonic=lambda: owner.now, time=lambda: next(wall_calls))
  class Carrot(actual_carrot):
    def update(self, sm, *a, **kw):
      return super().update(sm, *a, **kw, start_context=contexts['start'], start_observation=contexts['start_observation'], prepare_context=contexts['prepare'], prepare_observation=contexts['prepare_observation'])
  names = ['carControl', 'carState', 'controlsState', 'liveParameters', 'radarState', 'liveTracks', 'modelV2', 'selfdriveState', 'carrotMan', 'livePose', 'carrotNavi']
  sm = owner.messaging.SubMaster(names, poll='modelV2', ignore_avg_freq=['radarState'])
  expected, actual = [], []
  original_new_message = owner.messaging.new_message
  publication_now = [0]
  def new_message(*a, **kw):
    result = original_new_message(*a, **kw)
    result.logMonoTime = publication_now[0]
    return result
  owner.messaging.new_message = new_message
  def hook(row, _geometry_sm, start_runtime, prepare_runtime, start_read, observation, context):
    nonlocal carrot, wall_calls
    if carrot is None:
      carrot = Carrot(signal_assist=False, start_guard=start_runtime.guard, stop_prepare=prepare_runtime.prepare)
      carrot.signal_assist = None
    packets = []
    for file in row['sent']:
      with log.Event.from_bytes(Path(file).read_bytes()) as packet:
        if packet.which() in names:
          packets.append(packet.as_builder().as_reader())
    received = next(v['receive_time'] for v in row['services'] if v['name'] == 'modelV2')
    owner.now = received
    sm.update_msgs(received, packets)
    # Policy input is the actual owner constraint timestamp. Separate original
    # acquisition methods already recomputed their earlier/later read clocks.
    owner.now = row['constraint']['time']
    wall_calls = iter(row.get('wall_calls', [1000.0]))
    contexts.update(start=start_read['context'], start_observation=start_read['observation'], prepare=context, prepare_observation=observation)
    owner.logs.clear()
    owner.store.operations.clear()
    long.update(sm, carrot)
    lat.update(sm, carrot)
    state = snapshot(long, lat, carrot, owner.logs, owner.store.operations)
    recorded = []
    if args.record_events:
      swaglog = sys.modules['openpilot.common.swaglog']
      saved_logger = swaglog.cloudlog
      swaglog.cloudlog = NS(event=lambda event, **fields: recorded.append(dict(event=event, fields=fields)))
      scope = start_runtime.record.__func__.__globals__
      saved_time = scope['time']
      inputs = [v['fields'] for v in row['events'] if v['event'] == 'signalStartRecordInput']
      assert len(inputs) <= 1
      clock_inputs = iter([v['now'] for v in inputs])
      scope['time'] = NS(monotonic=lambda: next(clock_inputs))
      try:
        # The production owner calls these methods itself when it owns the
        # readers. This fixture injects independently replayed reader contexts,
        # so execute the same original methods with their captured call inputs.
        values = inputs[0] if inputs else dict(speed=0., model_stop_distance=0.)
        if inputs:
          compare(values['speed'], float(sm['carState'].vEgo))
          compare(values['model_stop_distance'], float(carrot._stop_x_rl))
        start_runtime.record(carrot.start_decision, float(sm['carState'].vEgo), float(carrot._stop_x_rl))
        assert next(clock_inputs, None) is None
        prepare_runtime.record_prepare(carrot.prepare_decision)
      finally:
        scope['time'] = saved_time
        swaglog.cloudlog = saved_logger
    signal = sanitize(dict(state=dict(start=vars(carrot.start_guard), prepare=vars(carrot.stop_prepare)), start=dataclasses.asdict(carrot.start_decision), prepare=dataclasses.asdict(carrot.prepare_decision), control_active=carrot.prepare_control_active, suspended=signal_road_policy.signal_detection_suspended(sm), color=carrot.signal_observation_color, green=carrot.signal_green_active, constraint=dict(time=long._signal_prepare_time, previous=long._signal_prepare_previous, cap=long._signal_prepare_accel.cap)))
    native_path = next(Path(path) for name, path in row['publications'] if name == 'longitudinalPlan')
    with log.Event.from_bytes(native_path.read_bytes()) as packet:
      native_wire = packet.to_dict()
    publication_now[0] = native_wire['logMonoTime']
    timing = native_wire['longitudinalPlan']
    saved = long.mpc.solve_time
    long.mpc.solve_time = timing['solverExecutionTime']
    published = []
    try:
      long.publish(sm, NS(send=lambda name, value: published.append(value.to_dict())), carrot, planner_execution_time=timing['plannerExecutionTime'], fast_radar_execution_time=timing['fastRadarExecutionTime'])
    finally:
      long.mpc.solve_time = saved
    assert len(published) == 1
    expected_row = dict(owner=state, signal=signal, wire=published[0])
    if args.record_events:
      expected_row['records'] = recorded
    expected.append(json.loads(json.dumps(expected_row)))
    if not args.source_only:
      actual.append(dict(owner=row['owner'], signal=dict(state=row['signal_state'], start=row['start_decision'], prepare=row['prepare_decision'], control_active=row['control_active'], suspended=timing['signalDetectionSuspended'], color=row['color'], green=row['green'], constraint=row['constraint']), wire=native_wire))
      if args.record_events:
        actual[-1]['records'] = [v for v in row['events'] if v['event'] in ('signalStartGuard', 'signalStopPrepare')]
  acquisition.HOOK = hook
  sys.argv = [sys.argv[0], '--capture', str(args.capture), '--output', str(args.output / 'acquisition')]
  acquisition.main()
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.source_only:
    print(json.dumps(dict(frames=len(expected), source_only=True)))
    return
  (args.output / 'actual.json').write_text(json.dumps(actual))
  mismatches = difference(expected, actual)
  receipt = dict(frames=len(expected), mismatch_count=len(mismatches), source_commit=request['source_commit'], diagnostic_timing='captured native timing inputs, not a performance comparison')
  if not mismatches:
    receipt.update(compare(actual, expected))
  (args.output / 'differences.json').write_text(json.dumps(mismatches))
  (args.output / 'receipt.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))
  assert not mismatches, mismatches[:3]


if __name__ == '__main__':
  main()

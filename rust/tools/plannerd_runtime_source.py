"""Original plannerd.main versus Rust serial owner using original SubMaster transitions."""

import argparse
import ast
import importlib
import json
from pathlib import Path
import inspect
import subprocess
import types

from openpilot.cereal import log
from plannerd_owner_fixtures import car_params, messages, parameters, save
from plannerd_owner_loader import SourceOwners
from plannerd_owner_source import difference


class Finished(Exception):
  pass


def canonical(raw):
  with log.Event.from_bytes(raw) as reader:
    result = reader.to_dict()
  kind = next(key for key in ('longitudinalPlan', 'lateralPlan', 'driverAssistance') if key in result)
  if kind != 'driverAssistance':
    result[kind].pop('solverExecutionTime', None)
  return result


def fixtures(source, output, count, faults=False):
  first = messages(source.messaging.new_message, 45, 99.925)
  initial_live = source.messaging.new_message('liveTracks', logMonoTime=99_900_000_000, valid=True)
  initial_live.liveTracks.init('points', 0)
  frames = [
    {'time': 99.9, 'wall_time': 999.9, 'parameters': {}, 'packets': [save(output, initial_live)]},
    {'time': 99.925, 'wall_time': 999.925, 'parameters': {}, 'packets': [save(output, first['radarState'])]},
  ]
  for index in range(count):
    now = 100.0 + index * 0.05
    packets = messages(source.messaging.new_message, index, now)
    if faults and 30 <= index < 50:
      packets['carState'].carState.useLaneLineSpeed = 0.0
    if faults and index in (42, 43, 44):
      packets['modelV2'].modelV2.orientationRate.z = [float('nan')] * 33
    if faults and index in (55, 56, 57):
      packets['modelV2'].modelV2.acceleration.x = [float('nan')] * 33
      packets['selfdriveState'].selfdriveState.experimentalMode = True
    if 290 <= index < 345:
      for lead in (packets['radarState'].radarState.leadOne, packets['radarState'].radarState.leadTwo):
        lead.status = True
    frames.append({'time': now, 'wall_time': 1000.0 + index * 0.05, 'parameters': {}, 'packets': [save(output, value) for value in packets.values()]})
    if index < 2 or 150 <= index < 158:
      continue
    live = source.messaging.new_message('liveTracks', logMonoTime=int((now + 0.025) * 1e9), valid=index != 90)
    points = live.liveTracks.init('points', 2)
    for target, lead in zip(points, (packets['radarState'].radarState.leadOne, packets['radarState'].radarState.leadTwo), strict=True):
      target.trackId = max(0, lead.radarTrackId)
      target.measured = True
      target.radarSource = 'frontRadar'
      target.dRel = lead.dRel + 0.05
      target.vRel = lead.vRel
      target.aRel = lead.aLeadK - 0.1
      target.aLead = lead.aLeadK
      target.jLead = lead.jLead
    if index in (65, 66):
      live.logMonoTime -= 50_000_000
    frames.append({'time': now + 0.025, 'wall_time': 1000.0 + index * 0.05 + 0.025, 'parameters': {}, 'packets': [save(output, live)]})
  return frames


class SourceRun:
  def __init__(self, source, frames, cp, output):
    self.source, self.frames, self.output = source, frames, output
    self.index = -1
    self.records = []
    self.publications = []
    self.sm = None
    self.total = 0
    source.store.values['CarParams'] = cp.to_bytes()
    module = importlib.import_module('openpilot.common.realtime')
    path = Path('openpilot/common/realtime.py')
    node = next(node for node in ast.parse(path.read_text()).body if isinstance(node, ast.ClassDef) and node.name == 'Priority')
    namespace = {}
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), 'exec'), namespace)
    module.Priority = namespace['Priority']
    module.config_realtime_process = lambda cores, priority: self.scheduler.append([cores, priority])
    self.scheduler = []
    msg_path = Path('openpilot/cereal/messaging/__init__.py')
    node = next(node for node in ast.parse(msg_path.read_text()).body if isinstance(node, ast.FunctionDef) and node.name == 'log_from_bytes')
    import capnp

    scope = {'capnp': capnp, 'log': log, 'NO_TRAVERSAL_LIMIT': 2**64 - 1}
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(msg_path), 'exec'), scope)
    source.messaging.log_from_bytes = scope['log_from_bytes']
    original = source.messaging.SubMaster
    owner = self

    class Subscriber(original):
      def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        owner.sm = self

      def update(self, timeout=100):
        owner.next()
        packets = []
        for path in owner.frames[owner.index]['packets']:
          with log.Event.from_bytes(Path(path).read_bytes()) as value:
            packets.append(value.as_builder().as_reader())
        self.update_msgs(source.now, packets)

    class Publisher:
      def __init__(self, topics):
        owner.topics = topics

      def send(self, name, value):
        owner.publications.append((name, canonical(value.to_bytes())))

    source.messaging.SubMaster = Subscriber
    source.messaging.PubMaster = Publisher
    diagnostics = importlib.import_module('openpilot.common.runtime_diagnostics')
    diagnostics.time = types.SimpleNamespace(monotonic=lambda: source.now)
    self.original = importlib.import_module('openpilot.selfdrive.controls.plannerd')
    self.original.time = types.SimpleNamespace(monotonic=lambda: source.now, thread_time=lambda: 0.0)

  def next(self):
    if self.index >= 0:
      events = [(level, (args[0] % args[1:] if len(args) > 1 else args[0])) for level, args, _kwargs in self.source.logs if level in ('warning', 'info')]
      caller = inspect.currentframe().f_back.f_back.f_locals
      self.records.append(
        {
          'messages': self.publications.copy(),
          'logs': events,
          'parameters': self.source.store.operations.copy(),
          'trigger': caller['planning_trigger'],
          'longitudinal_run': bool(caller['run_longitudinal'] and self.sm.seen['modelV2']),
          'model_updated': bool(self.sm.updated['modelV2']),
        }
      )
      self.source.logs.clear()
      self.source.store.operations.clear()
      self.publications.clear()
    self.index += 1
    if self.index == len(self.frames):
      raise Finished()
    current = self.frames[self.index]
    self.source.now, self.source.wall = current['time'], current['wall_time']
    self.source.store.values.update({key: value.encode() for key, value in current['parameters'].items()})

  def run(self):
    try:
      self.original.main()
    except Finished:
      pass
    assert self.scheduler == [[4, 51]], self.scheduler
    assert self.topics == ['longitudinalPlan', 'driverAssistance', 'lateralPlan'], self.topics
    # Startup records precede the first SubMaster iteration in both runtimes.
    self.records[0]['logs'] = [entry for entry in self.records[0]['logs'] if not entry[1].startswith('plannerd ')]
    return self.records


def main():
  parser = argparse.ArgumentParser()
  for key in ('source-native', 'artifact', 'binary', 'output'):
    parser.add_argument('--' + key, type=Path, required=True)
  parser.add_argument('--frames', type=int, default=360)
  parser.add_argument('--brand', default='hyundai', choices=('hyundai', 'volkswagen'))
  parser.add_argument('--radar-mode', default='1')
  parser.add_argument('--stock-longitudinal', action='store_true')
  parser.add_argument('--faults', action='store_true')
  parser.add_argument('--source-only', action='store_true')
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  values = parameters()
  values['EnableRadarTracks'] = args.radar_mode
  source = SourceOwners(args.source_native.resolve(), values)
  cp = car_params(args.brand)
  cp.openpilotLongitudinalControl = not args.stock_longitudinal
  cp_path = save(args.output, cp)
  frames = fixtures(source, args.output, args.frames, args.faults)
  source.now = frames[0]['time'] - 0.025
  source.wall = frames[0]['wall_time'] - 0.025
  expected = SourceRun(source, frames, cp, args.output).run()
  if args.faults:
    assert any('Lateral mpc - nan: True' in text for row in expected for _level, text in row['logs'])
    assert any('Long mpc reset, solution_status:' in text for row in expected for _level, text in row['logs'])
  request = {'artifact': str(args.artifact.resolve()), 'car_params': cp_path, 'parameters': values, 'output': str(args.output / 'rust'), 'frames': frames}
  encoded = json.dumps(request, allow_nan=False).encode()
  (args.output / 'input.json').write_bytes(encoded)
  (args.output / 'expected.json').write_text(json.dumps(expected, allow_nan=False) + '\n')
  if args.source_only:
    print(json.dumps({'source_polls': len(frames), 'source_messages': sum(len(row['messages']) for row in expected)}))
    return
  result = subprocess.run([str(args.binary.resolve()), str(args.output / 'actual.json')], input=encoded, capture_output=True, check=False)
  (args.output / 'stdout.txt').write_bytes(result.stdout)
  (args.output / 'stderr.txt').write_bytes(result.stderr)
  result.check_returncode()
  rows = json.loads((args.output / 'actual.json').read_bytes())
  actual = [
    {
      'messages': [(name, canonical(Path(path).read_bytes())) for name, path in row['messages']],
      'logs': row['logs'],
      'parameters': row['parameters'],
      'trigger': row['trigger'],
      'longitudinal_run': row['longitudinal_run'],
      'model_updated': row['model_updated'],
    }
    for row in rows
  ]
  expected = json.loads(json.dumps(expected))
  actual = json.loads(json.dumps(actual))
  mismatches = difference(expected, actual)
  report = {
    'frames': len(frames),
    'messages': sum(len(row['messages']) for row in expected),
    'mismatch_count': len(mismatches),
    'first_mismatches': mismatches[:100],
    'triggers': {name: sum(row['trigger'] == name for row in rows) for name in ('modelV2', 'liveTracks')},
  }
  (args.output / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n')
  assert not mismatches, mismatches[:2]
  print(json.dumps(report))


if __name__ == '__main__':
  main()

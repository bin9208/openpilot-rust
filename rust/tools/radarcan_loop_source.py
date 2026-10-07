from __future__ import annotations

import copy
from dataclasses import dataclass, field

from pytest import MonkeyPatch

from radarcan_decoder_source import snapshot
from radarcan_settings_source import settings
from radarcan_source import normalized


class Finished(Exception):
  pass


@dataclass
class Surface:
  case: dict
  tick: int = -1
  now_ns: int = 1_000_000_000
  publications: list = field(default_factory=list)
  errors: list[str] = field(default_factory=list)
  warnings: list[str] = field(default_factory=list)
  diagnostics: list = field(default_factory=list)
  constructors: list = field(default_factory=list)
  batches: list = field(default_factory=list)
  params_reads: list = field(default_factory=list)
  scheduling: list = field(default_factory=list)

  def poll(self, timeout: int) -> list:
    assert timeout == 20
    self.tick += 1
    if self.tick == len(self.case['ticks']):
      raise Finished
    self.now_ns = self.case['ticks'][self.tick]['now_ns']
    return []

  def drain(self, topic: str) -> list[bytes]:
    from openpilot.cereal import messaging
    tick = self.case['ticks'][self.tick]
    output = []
    for item in tick[topic]:
      if isinstance(item, list):
        output.append(bytes(item))
        continue
      if topic == 'can':
        event = messaging.new_message('can', len(item['frames']), valid=item.get('valid', True), logMonoTime=item['mono_time'])
        for index, frame in enumerate(item['frames']):
          event.can[index] = {'address': frame['address'], 'dat': bytes(frame['data']), 'src': frame['bus']}
      else:
        event = messaging.new_message('carState', valid=item.get('valid', True), logMonoTime=item['receive_ns'])
        event.carState.vEgo, event.carState.aEgo = item['v_ego'], item['a_ego']
        event.carState.radarInput = {'firstCanMonoTime': item['first_can_ns'], 'lastCanMonoTime': item['last_can_ns'],
          'canPacketCount': item['packet_count'], 'receiveMonoTime': item['receive_ns']}
      output.append(event.to_bytes())
    return output

  def send(self, topic, event) -> None:
    assert topic == 'liveTracks'
    self.publications.append(copy.deepcopy(event.to_dict()))


def trace(case):
  from openpilot.cereal import car
  from openpilot.selfdrive.carrot.radar import radarcan
  from opendbc.can.dbc import DBC
  from opendbc.car.car_helpers import interfaces
  from opendbc.car.carlog import carlog
  from radarcan_decoder_source import parser_snapshot
  surface = Surface(case)
  cp = car.CarParams.new_message(carFingerprint=case['candidate'], radarDelay=case['delay'], radarTimeStep=case['period'],
    radarUnavailable=case['unavailable'], flags=case.get('flags', 0), extFlags=case.get('ext_flags', 0),
    safetyConfigs=[{} for _ in range(case.get('safety_count', 1))])

  class Params:
    def get(self, key, block=False):
      surface.params_reads.append({'key': key, 'block': block})
      assert key == 'CarParams' and block
      return cp.to_bytes()

    def get_bool(self, key):
      surface.params_reads.append({'key': key})
      assert key == 'RadarTrackFlip'
      return case.get('flip', False)

  class Diagnostics:
    def __init__(self, component, emit):
      assert component == 'radarcan'

    def record(self, **values):
      surface.diagnostics.append(values)

  base_batches = radarcan.RadarCanBatches

  class Batches(base_batches):
    def __init__(self):
      super().__init__()
      surface.batches.append(self)

  original = interfaces[case['candidate']].RadarInterface

  class Observed(original):
    def __init__(self, parameters):
      super().__init__(parameters)
      surface.constructors.append(self)

    def update_carrot(self, *args):
      result = super().update_carrot(*args)
      surface.now_ns += case['ticks'][surface.tick].get('processing_ns', 0)
      return result

  DBC.cache_clear()
  failure = None
  with settings(case.get('params')) as integer_reads, MonkeyPatch.context() as fixture:
    fixture.setattr('opendbc.can.dbc.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.car.hyundai.radar_interface.DBC_PATH', case['dbc_root'])
    fixture.setattr(radarcan, 'Params', Params)
    fixture.setattr(radarcan, 'RadarCanBatches', Batches)
    fixture.setattr(radarcan, 'RuntimeDiagnostics', Diagnostics)
    fixture.setattr(radarcan, 'config_realtime_process', lambda core, priority: surface.scheduling.append([core, priority]))
    fixture.setattr(interfaces[case['candidate']], 'RadarInterface', Observed)
    fixture.setattr(radarcan.time, 'monotonic_ns', lambda: surface.now_ns)
    fixture.setattr(radarcan.time, 'monotonic', lambda: surface.now_ns * 1e-9)
    fixture.setattr(radarcan.time, 'thread_time', lambda: 0.)
    fixture.setattr(radarcan.cloudlog, 'error', surface.errors.append)
    fixture.setattr(carlog, 'warning', surface.warnings.append)
    fixture.setattr(radarcan.cloudlog, 'event', lambda *args, **kwargs: None)
    fixture.setattr(radarcan.messaging, 'Poller', lambda: surface)
    fixture.setattr(radarcan.messaging, 'sub_sock', lambda topic, **kwargs: topic)
    fixture.setattr(radarcan.messaging, 'PubMaster', lambda topics: surface)
    fixture.setattr(radarcan.messaging, 'drain_sock_raw', surface.drain)
    if case.get('replay', False):
      fixture.setenv('REPLAY', '1')
    else:
      fixture.delenv('REPLAY', raising=False)
    try:
      radarcan.main()
    except Finished:
      pass
    except Exception as error:
      failure = {'kind': type(error).__name__, 'message': str(error), 'tick': surface.tick}
    batches = surface.batches[0]
    states = [snapshot(state) for state in surface.constructors]
    return normalized({'publications': surface.publications, 'errors': surface.errors, 'warnings': surface.warnings, 'diagnostics': surface.diagnostics,
      'constructor_states': states, 'constructor_count': len(states), 'params_reads': surface.params_reads,
      'integer_reads': integer_reads, 'scheduling': surface.scheduling, 'pending_can': list(batches.can),
      'pending_states': [vars(state) for state in batches.states], 'overflowed': batches.overflowed, 'failure': failure,
      'parser': parser_snapshot(surface.constructors[-1].rcp)})

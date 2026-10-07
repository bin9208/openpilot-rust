from __future__ import annotations

import ctypes
import datetime
import os
from pathlib import Path
import sys
import time
from types import SimpleNamespace

from openpilot.cereal import log
from msgq.visionipc import VisionIpcClient, VisionStreamType
from runtime_peer import SERVICES
from qa_shapes import Camera as CameraProtocol, CameraSnapshot, Parameters, State, Road, Scene


def source_float(params: Parameters, key: str) -> float:
  value = params.get(key)
  if value is None or value == '':
    return 0.0
  data = value.encode() if isinstance(value, str) else value
  libc = ctypes.CDLL(None, use_errno=True)
  libc.strtof.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
  libc.strtof.restype = ctypes.c_float
  buffer = ctypes.create_string_buffer(data)
  end = ctypes.c_void_p()
  ctypes.set_errno(0)
  result = libc.strtof(buffer, ctypes.byref(end))
  if end.value == ctypes.addressof(buffer) or ctypes.get_errno() != 0:
    raise ValueError('invalid or out-of-range Params float')
  return result


def create(scene: Scene, ui: State) -> tuple[Road, CameraProtocol]:
  import openpilot.selfdrive.ui.ui_state as state

  state.UIStatus = SimpleNamespace(DISENGAGED=0, ENGAGED=1, OVERRIDE=2)
  sys.modules['openpilot.system.hardware'].TICI = False
  sys.modules['openpilot.system.hardware'].__path__ = [str(Path(__file__).resolve().parents[3] / 'openpilot/system/hardware')]
  type(ui.params).get_float = source_float
  ui.sm.recv_frame = dict.fromkeys(SERVICES, 0)
  ui.sm.recv_time = dict.fromkeys(SERVICES, 0.0)
  ui.sm.alive = dict.fromkeys(SERVICES, False)
  ui.sm.valid = dict.fromkeys(SERVICES, False)
  ui.sm.updated = dict.fromkeys(SERVICES, False)
  ui.sm.seen = dict.fromkeys(SERVICES, False)
  for name in SERVICES:
    event = log.Event.new_message()
    event.init(name, 0) if name in ['pandaStates', 'onroadEvents', 'customReservedRawData0'] else event.init(name)
    value = getattr(event, name)
    ui.sm[name] = list(value) if name in ['pandaStates', 'onroadEvents'] else bytes(value) if name == 'customReservedRawData0' else value
  if scene['config']['big']:
    from openpilot.selfdrive.ui.onroad.augmented_road_view import AugmentedRoadView
    import openpilot.selfdrive.ui.onroad.hud_renderer as hud

    hud.time.localtime = lambda stamp=None: (
      datetime.datetime(2026, 10, 1, 12, 34, 56).timetuple() if stamp is None else datetime.datetime.fromtimestamp(stamp).timetuple()
    )
    hud.time.time = lambda: datetime.datetime(2026, 10, 1, 12, 34, 56).timestamp()
  else:
    from openpilot.selfdrive.ui.mici.onroad.augmented_road_view import AugmentedRoadView
    import openpilot.selfdrive.ui.mici.onroad.hud_renderer as hud

  class FixedDatetime(datetime.datetime):
    @classmethod
    def now(cls, tz=None):
      return datetime.datetime(2026, 10, 1, 12, 34, 56)

  hud.datetime = FixedDatetime
  ui.update_params()
  ui.is_metric = ui.params.get_bool('IsMetric')
  ui.always_on_dm = ui.params.get_bool('AlwaysOnDM')
  ui.recording_audio = False
  ui.started_frame = 0
  ui.started_time = 0.0
  ui.panda_type = 1
  ui.status = 0
  ui.lat_active = False
  ui.params_memory = SimpleNamespace(get=lambda key, **kwargs: scene.get('memory', {}).get(key))
  widget = AugmentedRoadView()
  widget._name = 'rustvision'
  widget.client = VisionIpcClient('rustvision', VisionStreamType.VISION_STREAM_ROAD, conflate=True)

  class Camera:
    def before(self, index: int) -> None:
      directory = Path(os.environ['UI_CAMERA_SYNC'])
      (directory / f'{index}.ready').write_text('ready')
      start = time.perf_counter()
      while not (directory / f'{index}.allow').exists():
        assert time.perf_counter() - start < 15
        time.sleep(0.001)

    def snapshot(self) -> CameraSnapshot:
      return {
        'frame': widget.client.frame_id if widget.frame else None,
        'stream': int(widget.stream_type),
        'streams': sorted(int(value) for value in widget.available_streams),
        'mode': 0 if scene['config']['big'] else widget._road_view_mode(),
        'position': ui.params.get('DevicePosition') or '',
      }

  return widget, Camera()


def before(scene: Scene, ui: State, widget: Road, index: int) -> None:
  step = next(step for step in reversed(scene['road']['steps']) if step['frame'] <= index)
  scene.setdefault('params', {}).update(step.get('params', {}))
  scene.setdefault('memory', {}).update(step.get('memory', {}))
  ui.update_params()
  ui.is_metric = ui.params.get_bool('IsMetric')
  ui.always_on_dm = ui.params.get_bool('AlwaysOnDM')
  ui.started = ui.ignition = step['started']
  ui.status = step.get('status', 0)
  ui.engaged = ui.status == 1
  ui.lat_active = step.get('lat_active', False)
  ui.started_frame = step.get('started_frame', 0)
  ui.started_time = step.get('started_time', 0.0)
  ui.sm.updated = dict.fromkeys(SERVICES, False)
  for packet in step['messages']:
    with log.Event.from_bytes(bytes(packet)) as event:
      name = event.which()
      value = getattr(event, name)
      ui.sm[name] = list(value) if name in ['pandaStates', 'onroadEvents'] else bytes(value) if name == 'customReservedRawData0' else value.as_builder()
      ui.sm.recv_frame[name] = index * 2 + 3
      ui.sm.recv_time[name] = index / 20
      ui.sm.alive[name] = ui.sm.seen[name] = True
      ui.sm.valid[name] = event.valid
      ui.sm.updated[name] = True
  widget.set_cluster_hud_connected(step.get('suppress', False), False)

import datetime
from pathlib import Path
import sys
from types import SimpleNamespace
from openpilot.cereal import log

SERVICES = ['carState', 'controlsState', 'selfdriveState', 'carControl', 'liveParameters',
            'carOutput', 'longitudinalPlan', 'carrotMan', 'carrotNavi', 'deviceState', 'onroadEvents',
            'peripheralState', 'gpsLocationExternal', 'modelV2', 'radarState']


def create(scene, ui):
  import openpilot.selfdrive.ui.ui_state as state
  state.UIStatus = SimpleNamespace(DISENGAGED=0, ENGAGED=1, OVERRIDE=2)
  sys.modules['openpilot.system.hardware'].__path__ = [str(Path(__file__).resolve().parents[3] / 'openpilot/system/hardware')]
  if scene['config']['big']:
    import openpilot.selfdrive.ui.onroad.hud_renderer as module
    module.time.localtime = lambda stamp=None: datetime.datetime(2026, 10, 1, 12, 34, 56).timetuple() if stamp is None else datetime.datetime.fromtimestamp(stamp).timetuple()
    module.time.time = lambda: datetime.datetime(2026, 10, 1, 12, 34, 56).timestamp()
  else:
    import openpilot.selfdrive.ui.mici.onroad.hud_renderer as module

  class FixedDatetime(datetime.datetime):
    @classmethod
    def now(cls, tz=None):
      return datetime.datetime(2026, 10, 1, 12, 34, 56)

  module.datetime = FixedDatetime
  ui.update_params()
  ui.is_metric = ui.params.get_bool('IsMetric')
  ui.status = state.UIStatus.DISENGAGED
  ui.started_frame = 0
  ui.lat_active = False
  ui.sm.recv_frame = {name: 0 for name in SERVICES}
  ui.sm.alive = {name: False for name in SERVICES}
  ui.sm.valid = {name: False for name in SERVICES}
  ui.sm.updated = {name: False for name in SERVICES}
  for name in SERVICES:
    event = log.Event.new_message()
    event.init(name, 0) if name == 'onroadEvents' else event.init(name)
    ui.sm[name] = getattr(event, name)
  ui.params_memory = SimpleNamespace(get=lambda key, **kwargs: scene.get('memory', {}).get(key))
  widget = module.HudRenderer()
  widget._debug_speed_panel = scene['hud'].get('debug_speed', False)
  widget._debug_traffic_light = scene['hud'].get('debug_traffic', False)
  widget.owned_big = scene['config']['big']
  return widget


def before(scene, ui, widget, index):
  step = next(step for step in reversed(scene['hud']['steps']) if step['frame'] <= index)
  scene.setdefault('params', {}).update(step.get('params', {}))
  scene.setdefault('memory', {}).update(step.get('memory', {}))
  ui.update_params()
  ui.is_metric = ui.params.get_bool('IsMetric')
  ui.started_frame = step.get('started_frame', 0)
  ui.lat_active = step.get('lat_active', False)
  if not widget.owned_big:
    widget.set_wheel_critical_icon(step.get('critical', False))
    widget.set_can_draw_top_icons(step.get('top_icons', False))
  for message in step['messages']:
    with log.Event.from_bytes(bytes(message)) as event:
      name = event.which()
      value = getattr(event, name)
      ui.sm[name] = list(value) if name == 'onroadEvents' else value.as_builder()
      ui.sm.recv_frame[name] = index * 2 + 3
      ui.sm.alive[name] = True
      ui.sm.valid[name] = event.valid
      ui.sm.updated[name] = True


def snapshot(widget):
  if widget.owned_big:
    return {'cruise_set': widget.is_cruise_set, 'cruise_available': widget.is_cruise_available,
            'set_speed': widget.set_speed, 'speed': widget.speed, 'cluster_seen': widget.v_ego_cluster_seen,
            'engaged': widget._engaged, 'animation_text': widget._cruise_speed_animation_text,
            'animation_time': widget._cruise_speed_animation_time, 'blink': widget._blink_timer, 'display': widget._disp_timer,
            'settings': [widget._show_device_state, widget._show_date_time, widget._show_tpms,
                         widget._show_plot_mode, widget._longitudinal_personality], 'params_next': widget._hud_params_next_refresh_time}
  turn = widget._turn_intent
  return {'cruise_set': widget.is_cruise_set, 'cruise_available': widget.is_cruise_available,
          'set_speed': widget.set_speed, 'changed': widget._set_speed_changed_time, 'speed': widget.speed,
          'cluster_seen': widget.v_ego_cluster_seen, 'engaged': widget._engaged,
          'torque': widget._torque_filter.x, 'wheel_alpha': widget._wheel_alpha_filter.x,
          'animation_text': widget._cruise_speed_animation_text, 'animation_time': widget._cruise_speed_animation_time,
          'turn': {'pre': turn._pre, 'direction': turn._turn_intent_direction,
                   'alpha': turn._turn_intent_alpha_filter.x, 'rotation': turn._turn_intent_rotation_filter.x}}

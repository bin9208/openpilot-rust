import datetime
import ast
import subprocess
from pathlib import Path
import sys
from types import SimpleNamespace
from openpilot.cereal import log

SERVICES = ['carState', 'controlsState', 'selfdriveState', 'carControl', 'liveParameters',
            'carOutput', 'longitudinalPlan', 'carrotMan', 'carrotNavi', 'deviceState', 'onroadEvents',
            'peripheralState', 'gpsLocationExternal', 'modelV2', 'radarState']


def preserve_fixes(scene,ui,module):
  if scene.get('preserve_native_fixes'):
    # Explicit target composition: final upstream UI plus the isolated baseline's
    # existing navigation-owner labels and unknown-limit display. The raw final
    # source suite remains available and its measured differences are retained.
    base='b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'
    path='openpilot/selfdrive/ui/'+('onroad' if scene['config']['big'] else 'mici/onroad')+'/hud_renderer.py'
    tree=ast.parse(subprocess.check_output(['git','show',base+':'+path]))
    from openpilot.selfdrive.carrot.deceleration_source import navigation_status_presentation
    module.navigation_status_presentation=navigation_status_presentation
    for cls in tree.body:
      if not isinstance(cls,ast.ClassDef):continue
      for method in cls.body:
        if not isinstance(method,ast.FunctionDef):continue
        retained=cls.name=='SetSpeedOverride' and method.name=='compute'
        retained |= scene['config']['big'] and cls.name=='HudRenderer' and method.name in ('_draw_carrot_lower_status','_draw_carrot_speed_limit_box')
        if retained:
          scope=dict(module.__dict__)
          exec(compile(ast.Module(body=[method],type_ignores=[]),base+':'+path,'exec'),scope)
          setattr(getattr(module,cls.name),method.name,scope[method.name])
    if not scene['config']['big']:
      # Compact HUD retains the owner/lifecycle-aware presentation while all new
      # suspension, animation and badge code remains the final upstream class.
      original=module.navigation_status_presentation
      def owned_navigation(vehicle,external):
        man=ui.sm['carrotMan']
        return original(vehicle,external,str(getattr(man,'naviOwner','')),str(getattr(man,'naviLifecycle','')))
      module.navigation_status_presentation=owned_navigation



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

  preserve_fixes(scene,ui,module)

  class FixedDatetime(datetime.datetime):
    @classmethod
    def now(cls, tz=None):
      return datetime.datetime(2026, 10, 1, 12, 34, 56)

  module.datetime = FixedDatetime
  ui.update_params()
  badges(scene,ui)
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
  if scene['hud'].get('overlay'):widget._overlay_state=SimpleNamespace(key=None,enabled=True,urgent=False)
  return widget


def before(scene, ui, widget, index):
  if scene['hud'].get('overlay'):
    widget._overlay_state.enabled=index%19!=18
    widget._overlay_state.urgent=index%11==7
  step = next(step for step in reversed(scene['hud']['steps']) if step['frame'] <= index)
  scene.setdefault('params', {}).update(step.get('params', {}))
  scene.setdefault('memory', {}).update(step.get('memory', {}))
  ui.update_params()
  badges(scene,ui)
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

def badges(scene,ui):
  for model_field,ui_field in [('delivery_badge','usbgpu_delivery_badge'),('recovery_badge','usbgpu_recovery_badge'),('jetlink_badge','jetlink_badge')]:
    value=scene.get('models',{}).get(model_field)
    setattr(ui,ui_field,None if value is None else (value['text'],value['style']))

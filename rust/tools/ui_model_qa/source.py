"""Run unchanged ModelRenderer classes and original SubMaster on owned inputs."""

import ast
import colorsys
from dataclasses import dataclass, field
from enum import Enum
import json
import math
import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
import typing
import numpy as np
from capture import Capture
from snapshot import snapshot
from scenes import SERVICES

suite = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2])
output.mkdir(parents=True, exist_ok=True)
big = suite['config']['big']
os.environ.update(BIG='1' if big else '0', SCALE=str(suite['config']['scale']), OFFSCREEN='1')
hardware = ModuleType('openpilot.system.hardware')
hardware.PC, hardware.TICI, hardware.HARDWARE = True, suite['config']['large_viewport'], SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://model-source', swaglog_root=lambda: str(output / 'logs'))
sys.modules[paths.__name__] = paths
current_params = {}
current_param_reads = []


class Params:
  def __init__(self, *args):
    pass

  def get(self, key, *args, **kwargs):
    current_param_reads.append(key)
    raw = current_params.get(key)
    return raw if key == 'CarParams' or raw is None else raw.decode()

  def get_int(self, key):
    current_param_reads.append(key)
    return int(current_params.get(key, b'0') or b'0')

  def get_bool(self, key):
    current_param_reads.append(key)
    return current_params.get(key) == b'1'


params_module = ModuleType('openpilot.common.params')
params_module.Params, params_module.UnknownKeyName = Params, KeyError
sys.modules[params_module.__name__] = params_module
import pyray as rl
from openpilot.cereal import car, log
from openpilot.common.filter_simple import FirstOrderFilter
from openpilot.system.ui.lib.application import gui_app, FontWeight
from openpilot.system.ui.lib import shader_polygon, text_draw
from openpilot.system.ui.lib.multilang import multilang
from openpilot.system.ui.widgets import Widget
from openpilot.selfdrive.ui.onroad.path_geometry import sample_path, project_path
from openpilot.selfdrive.ui.road_markings import lane_dash_segments, project_lane_segments, project_blindspot_barrier, blindspot_barrier_quads
from openpilot.selfdrive.ui.render_diagnostics import RenderDiagnostics

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_message_state import source as source_messaging

scope, _ = source_messaging()


class UIStatus(Enum):
  DISENGAGED = 'disengaged'
  ENGAGED = 'engaged'
  OVERRIDE = 'override'


ui = SimpleNamespace(params=Params())


def log_from_bytes(value, schema):
  with schema.from_bytes(value) as message:
    return message


from openpilot.selfdrive.ui.mici.onroad import blend_colors


multilang._language = suite['language']
multilang.setup()
gui_app.init_window('Source model renderer')
record = Capture(rl, shader_polygon, text_draw)
module = ModuleType('source_model_renderer')
sys.modules[module.__name__] = module
module.__dict__.update(
  math=math,
  colorsys=colorsys,
  np=np,
  rl=rl,
  dataclass=dataclass,
  field=field,
  FirstOrderFilter=FirstOrderFilter,
  Params=Params,
  HEIGHT_INIT=np.array([1.22]),
  ui_state=ui,
  UIStatus=UIStatus,
  gui_app=gui_app,
  FontWeight=FontWeight,
  draw_text_ui_style=text_draw.draw_text_ui_style,
  draw_polygon=record.shaded,
  draw_polygon_solid=record.solid,
  Gradient=shader_polygon.Gradient,
  Widget=Widget,
  LaneChangeState=log.LaneChangeState,
  car=car,
  log=log,
  messaging=SimpleNamespace(log_from_bytes=log_from_bytes),
  project_path=project_path,
  sample_path=sample_path,
  lane_dash_segments=lane_dash_segments,
  project_lane_segments=project_lane_segments,
  project_blindspot_barrier=project_blindspot_barrier,
  blindspot_barrier_quads=blindspot_barrier_quads,
  Optional=typing.Optional,
  Any=typing.Any,
  blend_colors=blend_colors,
  RenderDiagnostics=lambda component: RenderDiagnostics(component, emit=lambda *args, **kwargs: None),
)
path = Path('openpilot/selfdrive/ui') / ('onroad' if big else 'mici/onroad') / 'model_renderer.py'
tree = ast.parse(path.read_text())
nodes = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom))]
exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), module.__dict__)
now = 0.0
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
loop = gui_app.render()
results = []
try:
  for scene in suite['cases']:
    current_params.clear()
    current_params.update({k: bytes(v) for k, v in scene['params'].items()})
    ui.sm = scope['SubMaster'](SERVICES)
    current_param_reads.clear()
    widget = module.ModelRenderer()
    rect = rl.Rectangle(*(scene['rect'][key] for key in ['x', 'y', 'width', 'height']))
    widget.set_rect(rect)
    rows = []
    for index, step in enumerate(scene['steps']):
      now = step['now']
      current_params.update({k: bytes(v) for k, v in step['params'].items()})
      ui.started_frame, ui.lat_active = step['ui']['started_frame'], step['ui']['lat_active']
      ui.status = UIStatus(step['ui']['status'])
      ui.is_metric, ui.show_radar_info = step['ui']['is_metric'], step['ui']['show_radar_info']
      messages = [log_from_bytes(bytes(packet), log.Event) for packet in step['messages']]
      ui.sm.update_msgs(now, messages)
      if 'transform' in step:
        widget.set_transform(np.asarray(step['transform'], dtype=np.float64))
      next(loop)
      record.commands = []
      widget._render(rect)
      rows.append({'state': snapshot(widget, big), 'commands': record.commands})
      if step['capture']:
        rl.rl_draw_render_batch_active()
        image = rl.load_image_from_screen()
        assert rl.export_image(image, str(output / f"{scene['name']}-{index}.png"))
        rl.unload_image(image)
    results.append({'name': scene['name'], 'rows': rows, 'parameter_reads': current_param_reads.copy()})
  (output / 'trace.json').write_text(json.dumps(results) + '\n')
finally:
  loop.close()
  shader_polygon.cleanup_shader_resources()
  gui_app.close()

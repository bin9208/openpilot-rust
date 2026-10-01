import json
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
import numpy as np
import pyray as rl

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2])
app = ModuleType('openpilot.system.ui.lib.application')
app.gui_app = SimpleNamespace(width=536, height=240)
app.GL_VERSION = '#version 300 es\nprecision highp float;\n'
sys.modules[app.__name__] = app
from openpilot.system.ui.lib import shader_polygon as polygon

rl.set_config_flags(32)
rl.init_window(536, 240, 'Source ribbon polygons')
rl.set_target_fps(0)
points = np.asarray([[point['x'], point['y']] for point in scene['points']], dtype=np.float32)
origin = rl.Rectangle(*(scene['origin'][key] for key in ('x', 'y', 'width', 'height')))
colors = [rl.Color(*int(color).to_bytes(4, 'little')) for color in scene['colors']]
gradient = polygon.Gradient(scene['start'], scene['end'], colors, scene['stops'])
failed = False
for index in range(3):
  rl.begin_drawing()
  rl.clear_background(rl.BLACK)
  try:
    if scene['mode'] == 'gradient':
      polygon.draw_polygon(origin, points, gradient=gradient)
    elif scene['mode'] == 'color':
      polygon.draw_polygon(origin, points, color=colors[0])
    elif scene['mode'] == 'solid':
      polygon.draw_polygon_solid(points, colors[0])
  except AttributeError:
    if not scene.get('expect_error', False):
      raise
    failed = True
  if index == 2:
    rl.rl_draw_render_batch_active()
    capture = rl.load_image_from_screen()
    assert rl.export_image(capture, str(output))
    rl.unload_image(capture)
  rl.end_drawing()
output.with_suffix('.json').write_text(
  json.dumps(
    {
      'failed': failed,
      'strip': [{'x': x, 'y': y} for x, y in polygon.triangulate(points)],
      'stops': gradient.stops,
      'colors': [int.from_bytes(bytes([color.r, color.g, color.b, color.a]), 'little') for color in gradient.colors],
    }
  )
)
polygon.cleanup_shader_resources()
rl.close_window()

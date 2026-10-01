"""Unchanged product QRCodeTexture lifecycle and real native GL image upload."""

import os
from pathlib import Path
import sys
import json
from types import ModuleType, SimpleNamespace

os.environ.update(BIG='0', SCALE='1', OFFSCREEN='1')
output = Path(sys.argv[1]).resolve()
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = False
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://qr-source', swaglog_root=lambda: str(output.parent / 'logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app
from openpilot.selfdrive.ui.widgets.qr_code import QRCodeTexture

texture = None
try:
  gui_app.init_window('QR source')
  texture = QRCodeTexture()
  states = []
  for value in ['http://192.0.2.4:7000', 'http://192.0.2.4:7000', None, 'http://[2001:db8::1]:7000', 'http://192.0.2.5:7000']:
    states.append({'changed': texture.set_data(value), 'available': texture.available})
  loop = gui_app.render()
  next(loop)
  rl.draw_rectangle_rec(rl.Rectangle(0, 0, 536, 240), rl.Color(38, 38, 38, 255))
  texture.render(rl.Rectangle(168, 20, 200, 200))
  rl.rl_draw_render_batch_active()
  image = rl.load_image_from_screen()
  assert rl.export_image(image, str(output))
  rl.unload_image(image)
  texture.destroy()
  states.append({'available': texture.available})
  output.with_suffix('.json').write_text(json.dumps(states))
  loop.close()
finally:
  if texture is not None:
    texture.destroy()
  gui_app.close()

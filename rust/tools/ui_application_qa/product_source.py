"""Actual product widget render oracle with owned data boundaries."""

import json
import os
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ.update(BIG='1' if scene['config']['big'] else '0', SCALE=str(scene['config']['scale']), OFFSCREEN='1')
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = scene['config']['large_viewport']
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://product-source', swaglog_root=lambda: str(output.parent / 'logs'))
sys.modules[paths.__name__] = paths
params_module = ModuleType('openpilot.common.params')


class Params:
  def get(self, key, *args, **kwargs):
    return scene.get('params', {}).get(key)

  def get_bool(self, key, *args):
    return self.get(key) == '1'

  def get_int(self, key, *args):
    return int(self.get(key) or 0)


params_module.Params = Params
sys.modules[params_module.__name__] = params_module
state_module = ModuleType('openpilot.selfdrive.ui.ui_state')
state_module.ui_state = SimpleNamespace(
  prime_state=SimpleNamespace(is_prime=lambda: scene['prime'] > 0, is_paired=lambda: scene['prime'] > -1),
  params_memory=SimpleNamespace(get=lambda key: scene.get('address')),
)
sys.modules[state_module.__name__] = state_module
import pyray as rl
from openpilot.system.ui.lib.application import gui_app
from openpilot.system.ui.lib.multilang import multilang
from openpilot.selfdrive.ui.widgets.prime import PrimeWidget
from openpilot.selfdrive.ui.widgets.carrot_web_dialog import CarrotWebDialog

multilang._language = scene['language']
multilang.setup()
gui_app.init_window('Source product widget')
if scene['kind'] == 'ssh':
  from openpilot.selfdrive.ui.widgets.ssh_key import SshKeyAction

  widget = SshKeyAction()
elif scene['kind'] == 'setup':
  from openpilot.selfdrive.ui.widgets.setup import SetupWidget

  widget = SetupWidget()
elif scene['kind'] == 'pairing':
  if scene['config']['big']:
    from openpilot.selfdrive.ui.widgets.pairing_dialog import PairingDialog
  else:
    from openpilot.selfdrive.ui.mici.widgets.pairing_dialog import PairingDialog
  widget = PairingDialog()
  widget._get_pairing_url = lambda: 'https://connect.comma.ai/?pair=fixture'
elif scene['kind'] == 'prime':
  widget = PrimeWidget()
else:
  widget = CarrotWebDialog()
  widget._session._timestamp_factory = lambda: '12:34:56'
now = 0.0
original = time.monotonic
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
rect = rl.Rectangle(*(scene['rect'][key] for key in ['x', 'y', 'width', 'height']))
widget.set_rect(rect)
widget.show_event()
loop = gui_app.render()
results = []
try:
  for index in range(scene['frames']):
    now = index / 20
    next(loop)
    widget.render()
    results.append({'prime': scene['prime']})
  rl.rl_draw_render_batch_active()
  image = rl.load_image_from_screen()
  assert rl.export_image(image, str(output))
  rl.unload_image(image)
  output.with_suffix('.json').write_text(json.dumps(results))
  loop.close()
finally:
  time.monotonic = original
  gui_app.close()

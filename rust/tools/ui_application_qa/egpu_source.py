import importlib.util
from pathlib import Path
import sys
import threading
import time
from types import ModuleType, SimpleNamespace


class Fixture:
  def __init__(self, options, output):
    self.options = options
    self.calls = 0
    self.release = threading.Event()
    self.path = output.with_suffix('.compiled.pkl')
    self.manifest = Path(str(self.path) + '.chunkmanifest')
    self.manifest.write_text('owned recompile fixture')

  def check(self):
    self.calls += 1
    assert self.release.wait(30), 'owned eGPU check was not released'
    return self.options.get('check_error')

  def before(self, index):
    if self.options.get('complete_at') == index:
      self.release.set()
      time.sleep(0.02)

  def snapshot(self):
    return {'calls': self.calls, 'removals': int(not self.manifest.exists())}


def prepare(scene, output):
  from openpilot.selfdrive.ui.ui_state import ui_state

  ui_state.update_params()
  root = Path(__file__).resolve().parents[3]
  spec = importlib.util.spec_from_file_location('openpilot.system.hardware.usbgpu', root / 'openpilot/system/hardware/usbgpu.py')
  module = importlib.util.module_from_spec(spec)
  sys.modules[spec.name] = module
  spec.loader.exec_module(module)
  fixture = Fixture(scene['egpu'], output)
  devices = [SimpleNamespace(**item) for item in scene['egpu'].get('devices', [])]
  module.get_usbgpu_devices = lambda *args: devices
  module.check_usbgpu = fixture.check
  helpers = ModuleType('openpilot.selfdrive.modeld.helpers')
  helpers.active_usbgpu_compiled_path = lambda: fixture.path
  sys.modules[helpers.__name__] = helpers
  return fixture


def create(scene, output):
  fixture = prepare(scene, output)
  if scene['config']['big']:
    from openpilot.selfdrive.ui.layouts.settings.usbgpu import UsbGpuLayout

    widget = UsbGpuLayout()
  else:
    from openpilot.selfdrive.ui.mici.layouts.settings.usbgpu import UsbGpuLayoutMici

    widget = UsbGpuLayoutMici()
  return widget, fixture

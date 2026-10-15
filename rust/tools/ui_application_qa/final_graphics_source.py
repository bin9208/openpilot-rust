"""Load the exact final graphics owners for source-only comparison processes."""
import subprocess
import sys
from pathlib import Path
from types import ModuleType

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'

def install(widgets=False):
  names=['openpilot.system.ui.lib.'+suffix for suffix in ['application','text_measure','text_draw','wrap_text']]
  names += ['openpilot.selfdrive.carrot.signal_mode', 'openpilot.selfdrive.carrot.signal_display', 'openpilot.selfdrive.ui.signal_status']
  if widgets:names.append('openpilot.system.ui.widgets.label')
  for name in names:
    suffix=name.rpartition('.')[2]
    path=name.replace('.', '/')+'.py'
    module=ModuleType(name)
    module.__file__=str(Path.cwd()/path)
    module.__package__=name.rpartition('.')[0]
    sys.modules[name]=module
    pin = '8cf5570f6962bd9cb17ec384bbfa12a3a167a5a8' if name in ('openpilot.selfdrive.carrot.signal_mode', 'openpilot.selfdrive.carrot.signal_display', 'openpilot.selfdrive.ui.signal_status') else PIN
    exec(compile(subprocess.check_output(['git','show',pin+':'+path]),pin+':'+path,'exec'),module.__dict__)
    parent=sys.modules.get(module.__package__)
    if parent is not None:setattr(parent,suffix,module)


def install_traffic_light():
  # Call after the comparison's UIState boundary is installed (ConfidenceBall
  # imports its UIStatus), before the HUD captures the TrafficLight class.
  import importlib
  name='openpilot.selfdrive.ui.mici.onroad.traffic_light'
  module=importlib.import_module(name)
  pin='8cf5570f6962bd9cb17ec384bbfa12a3a167a5a8'
  path=name.replace('.', '/')+'.py'
  exec(compile(subprocess.check_output(['git','show',pin+':'+path]),pin+':'+path,'exec'),module.__dict__)

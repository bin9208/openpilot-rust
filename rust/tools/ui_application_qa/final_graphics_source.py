"""Load the exact final graphics owners for source-only comparison processes."""
import subprocess
import sys
from pathlib import Path
from types import ModuleType

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'

def install(widgets=False):
  names=['openpilot.system.ui.lib.'+suffix for suffix in ['application','text_measure','text_draw','wrap_text']]
  if widgets:names.append('openpilot.system.ui.widgets.label')
  for name in names:
    suffix=name.rpartition('.')[2]
    path=name.replace('.', '/')+'.py'
    module=ModuleType(name)
    module.__file__=str(Path.cwd()/path)
    module.__package__=name.rpartition('.')[0]
    sys.modules[name]=module
    exec(compile(subprocess.check_output(['git','show',PIN+':'+path]),PIN+':'+path,'exec'),module.__dict__)
    parent=sys.modules.get(module.__package__)
    if parent is not None:setattr(parent,suffix,module)

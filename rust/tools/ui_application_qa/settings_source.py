import ast
from collections.abc import Callable
from dataclasses import dataclass
from enum import IntEnum
from pathlib import Path
import sys
from types import ModuleType
import pyray as rl
from device_source import device_layout, mici_device_layout
from egpu_source import prepare as prepare_egpu
from network_source import prepare as prepare_network


def create(scene, output, context):
  from openpilot.system.ui.lib.application import FontWeight, MousePos
  from openpilot.system.ui.lib.multilang import tr, tr_noop
  from openpilot.system.ui.lib.text_measure import measure_text_cached
  from openpilot.system.ui.widgets import Widget

  root = Path(__file__).resolve().parents[3]
  ui, gui = context.ui, context.gui
  manager = prepare_network(scene, ui)
  egpu = prepare_egpu(scene, output)
  module = ModuleType('owned_settings_root')
  sys.modules[module.__name__] = module
  namespace = module.__dict__
  namespace.update(
    rl=rl,
    dataclass=dataclass,
    IntEnum=IntEnum,
    Callable=Callable,
    gui_app=gui,
    FontWeight=FontWeight,
    MousePos=MousePos,
    tr=tr,
    tr_noop=tr_noop,
    measure_text_cached=measure_text_cached,
    Widget=Widget,
    ui_state=ui,
    Params=context.params,
  )
  if scene['config']['big']:
    from openpilot.selfdrive.ui.layouts.settings.developer import DeveloperLayout
    from openpilot.selfdrive.ui.layouts.settings.firehose import FirehoseLayout
    from openpilot.selfdrive.ui.layouts.settings.software import SoftwareLayout
    from openpilot.selfdrive.ui.layouts.settings.toggles import TogglesLayout
    from openpilot.selfdrive.ui.layouts.settings.usbgpu import UsbGpuLayout
    from openpilot.system.ui.widgets.network import NetworkUI

    device = device_layout(root, context.params, ui, gui)
    namespace.update(
      DeveloperLayout=DeveloperLayout,
      DeviceLayout=lambda: device,
      FirehoseLayout=FirehoseLayout,
      SoftwareLayout=SoftwareLayout,
      TogglesLayout=TogglesLayout,
      UsbGpuLayout=UsbGpuLayout,
      WifiManager=lambda: manager,
      NetworkUI=NetworkUI,
    )
    source = root / 'openpilot/selfdrive/ui/layouts/settings/settings.py'
  else:
    from openpilot.selfdrive.ui.mici.widgets.button import BigButton
    from openpilot.selfdrive.ui.mici.layouts.settings.toggles import TogglesLayoutMici
    from openpilot.selfdrive.ui.mici.layouts.settings.network.network_layout import NetworkLayoutMici
    from openpilot.selfdrive.ui.mici.layouts.settings.developer import DeveloperLayoutMici
    from openpilot.selfdrive.ui.mici.layouts.settings.firehose import FirehoseLayout
    from openpilot.selfdrive.ui.mici.layouts.settings.usbgpu import UsbGpuLayoutMici
    from openpilot.system.ui.widgets.scroller import NavScroller

    device = mici_device_layout(root, context.params, ui, gui, context.effects, scene)
    panels = {
      'Device': device,
      'Toggles': TogglesLayoutMici(),
      'Network': NetworkLayoutMici(),
      'Developer': DeveloperLayoutMici(),
      'Firehose': FirehoseLayout(),
      'Egpu': UsbGpuLayoutMici(),
    }
    for name, panel in panels.items():
      panel.owned_page = f'Settings({name})'
    namespace.update(
      BigButton=BigButton,
      NavScroller=NavScroller,
      DeviceLayoutMici=lambda: panels['Device'],
      PairBigButton=type(device).__init__.__globals__['PairBigButton'],
      TogglesLayoutMici=lambda: panels['Toggles'],
      NetworkLayoutMici=lambda: panels['Network'],
      DeveloperLayoutMici=lambda: panels['Developer'],
      FirehoseLayout=lambda: panels['Firehose'],
      UsbGpuLayoutMici=lambda: panels['Egpu'],
    )
    source = root / 'openpilot/selfdrive/ui/mici/layouts/settings/settings.py'
  nodes = [node for node in ast.parse(source.read_text()).body if isinstance(node, (ast.Assign, ast.ClassDef))]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(source), 'exec'), namespace)
  widget = namespace['SettingsLayout']()
  if scene['config']['big']:
    widget.set_callbacks(lambda: context.effects.effects.append({'page': 'Home'}))
  return widget, manager, egpu

from __future__ import annotations

import ast
from enum import IntEnum
import json
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace
import pyray as rl
from openpilot.cereal import messaging
from openpilot.system.ui.widgets import Widget
from openpilot.system.ui.widgets.scroller import Scroller
from openpilot.selfdrive.ui.carrot_param_cache import TimedSnapshotCache, read_screen_record
from road_source import create as road_create, before as road_before
from settings_source import create as settings_create
from qa_shapes import Context, Root, RootSnapshot, State, Scene, Camera


def create(scene: Scene, output: Path, context: Context) -> tuple[Root, Camera]:
  root = Path(__file__).resolve().parents[3]
  ui, gui = context.ui, context.gui
  road, camera = road_create(scene, ui)
  manager = ModuleType('openpilot.selfdrive.selfdrived.alertmanager')
  manager.OFFROAD_ALERTS = json.loads((root / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())
  sys.modules[manager.__name__] = manager
  settings, _, _ = settings_create(scene, output, context)
  ui.params_memory = SimpleNamespace(get=lambda key, **kwargs: scene.get('memory', {}).get(key))
  timeout = []
  context.device.add_interactive_timeout_callback = timeout.append
  from openpilot.selfdrive.ui.widgets.carrot_web_dialog import CarrotWebDialog

  namespace = {
    'rl': rl,
    'IntEnum': IntEnum,
    'gui_app': gui,
    'Widget': Widget,
    'ui_state': ui,
    'device': context.device,
    'time': time,
    'SettingsLayout': lambda: settings,
    'AugmentedRoadView': lambda *args, **kwargs: road,
    'OnboardingWindow': lambda *args: SimpleNamespace(completed=True),
    'CarrotWebDialog': CarrotWebDialog,
    'TimedSnapshotCache': TimedSnapshotCache,
    'read_screen_record': read_screen_record,
    'messaging': messaging,
    'Scroller': Scroller,
  }
  if scene['config']['big']:
    from openpilot.selfdrive.ui.layouts.sidebar import Sidebar, SIDEBAR_WIDTH
    from openpilot.selfdrive.ui.layouts.home import HomeLayout

    namespace.update(Sidebar=Sidebar, SIDEBAR_WIDTH=SIDEBAR_WIDTH, HomeLayout=HomeLayout, PanelType=type(settings._current_panel))
    source = root / 'openpilot/selfdrive/ui/layouts/main.py'
    name = 'MainLayout'
  else:
    from openpilot.selfdrive.ui.mici.layouts.home import MiciHomeLayout
    from openpilot.selfdrive.ui.mici.layouts.offroad_alerts import MiciOffroadAlerts
    from openpilot.selfdrive.ui.mici.onroad.debug_plot import DebugPlot

    namespace.update(MiciHomeLayout=MiciHomeLayout, MiciOffroadAlerts=MiciOffroadAlerts, DebugPlot=DebugPlot, ONROAD_DELAY=2.5)
    source = root / 'openpilot/selfdrive/ui/mici/layouts/main.py'
    name = 'MiciMainLayout'
  nodes = [node for node in ast.parse(source.read_text()).body if isinstance(node, ast.ClassDef)]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(source), 'exec'), namespace)
  widget = namespace[name]()
  widget.owned_timeout = timeout
  widget.owned_road = road
  return widget, camera


def before(scene: Scene, ui: State, widget: Root, index: int) -> None:
  road_before(scene, ui, widget.owned_road, index)
  step = next((step for step in scene['root'].get('steps', []) if step['frame'] == index), {})
  if 'page' in step:
    if scene['config']['big']:
      if step['page'] == 'home':
        widget._set_mode_for_state()
      else:
        panel = type(widget._layouts[1]._current_panel)[step['page'].upper()]
        widget.open_settings(panel)
    elif step['page'] == 'home':
      widget._in_plot_mode = False
      widget._scroll_to(widget._home_layout)
    else:
      from openpilot.system.ui.lib.application import gui_app

      gui_app.push_widget(widget._settings_layout)
  if step.get('timeout'):
    for callback in widget.owned_timeout:
      callback()


def snapshot(widget: Root, big: bool) -> RootSnapshot:
  from openpilot.system.ui.lib.application import gui_app

  if big:
    mode = widget._current_mode.name.title()
    panel = widget._layouts[1]._current_panel.name.title() if mode == 'Settings' else None
    return {
      'mode': mode,
      'settings_panel': panel,
      'sidebar': widget._sidebar.is_visible,
      'scroll': None,
      'in_plot_mode': False,
      'recording': gui_app.is_recording(),
      'stack': len(gui_app._nav_stack),
    }
  return {
    'mode': None,
    'settings_panel': None,
    'sidebar': False,
    'scroll': widget._scroller.scroll_panel.get_offset(),
    'in_plot_mode': widget._in_plot_mode,
    'recording': gui_app.is_recording(),
    'stack': len(gui_app._nav_stack),
  }

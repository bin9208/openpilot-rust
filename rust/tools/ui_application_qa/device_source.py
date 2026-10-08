"""Original device class with owned Params/cereal and unopened page factories."""

import ast
import math
from types import SimpleNamespace


def device_layout(root, params, ui, gui):
  from openpilot.cereal import log
  from openpilot.system.ui.lib.application import FontWeight
  from openpilot.system.ui.lib.multilang import multilang, tr, tr_noop
  from openpilot.system.ui.widgets import Widget, DialogResult
  from openpilot.system.ui.widgets.confirm_dialog import ConfirmDialog, alert_dialog
  from openpilot.system.ui.widgets.html_render import HtmlModal
  from openpilot.system.ui.widgets.list_view import text_item, button_item, dual_button_item
  from openpilot.system.ui.widgets.option_dialog import MultiOptionDialog
  from openpilot.system.ui.widgets.scroller_tici import Scroller

  def decode(data, struct):
    with struct.from_bytes(data) as value:
      return value

  def page(name):
    return lambda: SimpleNamespace(owned_page=name)

  namespace = {
    'log': log,
    'FontWeight': FontWeight,
    'multilang': multilang,
    'tr': tr,
    'tr_noop': tr_noop,
    'Widget': Widget,
    'DialogResult': DialogResult,
    'ConfirmDialog': ConfirmDialog,
    'alert_dialog': alert_dialog,
    'HtmlModal': HtmlModal,
    'text_item': text_item,
    'button_item': button_item,
    'dual_button_item': dual_button_item,
    'MultiOptionDialog': MultiOptionDialog,
    'Scroller': Scroller,
    'BASEDIR': str(root),
    'Params': params,
    'ui_state': ui,
    'gui_app': gui,
    'messaging': SimpleNamespace(log_from_bytes=decode),
    'math': math,
    'cloudlog': SimpleNamespace(exception=lambda *args: __import__('traceback').print_exc()),
    'DriverCameraDialog': page('DriverCamera'),
    'TrainingGuide': page('Training'),
    'PairingDialog': page('Pairing'),
    'os': __import__('os'),
  }
  source = ast.parse((root / 'openpilot/selfdrive/ui/layouts/settings/device.py').read_text())
  nodes = [node for node in source.body if isinstance(node, ast.ClassDef) or isinstance(node, ast.Assign)]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'device.py', 'exec'), namespace)
  return namespace['DeviceLayout']()


def mici_device_layout(root, params, ui, gui, effects, scene):
  import os
  import pyray as rl
  from enum import IntEnum
  from collections.abc import Callable
  from openpilot.system.ui.widgets.scroller import NavScroller
  from openpilot.selfdrive.ui.mici.widgets.button import BigButton, BigCircleButton
  from openpilot.selfdrive.ui.mici.widgets.dialog import BigDialog, BigConfirmationDialog
  from openpilot.system.ui.lib.application import FontWeight, MousePos
  from openpilot.system.ui.lib.multilang import tr
  from openpilot.system.ui.widgets import Widget
  from openpilot.system.ui.widgets.label import UnifiedLabel

  def page(name):
    return lambda *args, **kwargs: SimpleNamespace(owned_page=name)

  class Thread:
    def __init__(self, target, **kwargs):
      self.target = target

    def start(self):
      self.target()

  namespace = {
    'rl': rl,
    'IntEnum': IntEnum,
    'Callable': Callable,
    'NavScroller': NavScroller,
    'BigButton': BigButton,
    'BigCircleButton': BigCircleButton,
    'BigDialog': BigDialog,
    'BigConfirmationDialog': BigConfirmationDialog,
    'FontWeight': FontWeight,
    'MousePos': MousePos,
    'tr': tr,
    'Widget': Widget,
    'UnifiedLabel': UnifiedLabel,
    'BASEDIR': str(root),
    'Params': params,
    'ui_state': ui,
    'gui_app': gui,
    'UNREGISTERED_DONGLE_ID': 'UnregisteredDevice',
    'system_time_valid': lambda: scene.get('time_valid', True),
    'DriverCameraDialog': page('DriverCamera'),
    'ReviewTrainingGuide': page('Training'),
    'ReviewTermsPage': page('Terms'),
    'PairingDialog': page('Pairing'),
    'MiciFccModal': page('Regulatory'),
    'HtmlModal': object,
    'threading': SimpleNamespace(Thread=Thread),
    'os': SimpleNamespace(path=os.path, system=lambda command: effects.effects.append({'updater': 'Download' if 'SIGHUP' in command else 'Check'})),
  }
  source = ast.parse((root / 'openpilot/selfdrive/ui/mici/layouts/settings/device.py').read_text())
  names = {
    'DeviceInfoLayoutMici',
    'UpdaterState',
    'PairBigButton',
    'UpdateOpenpilotBigButton',
    'DeviceLayoutMici',
    'EngagedConfirmationCircleButton',
    'EngagedConfirmationButton',
    '_engaged_confirmation_click',
  }
  nodes = [node for node in source.body if getattr(node, 'name', None) in names or isinstance(node, ast.Assign)]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'mici/device.py', 'exec'), namespace)
  return namespace['DeviceLayoutMici']()


def regulatory(root, gui, big):
  from openpilot.system.ui.widgets.html_render import HtmlModal, HtmlRenderer

  if big:
    return HtmlModal(str(root / 'openpilot/selfdrive/assets/offroad/fcc.html'))
  import pyray as rl
  from openpilot.system.ui.widgets.scroller import NavRawScrollPanel

  source = ast.parse((root / 'openpilot/selfdrive/ui/mici/layouts/settings/device.py').read_text())
  cls = next(node for node in source.body if isinstance(node, ast.ClassDef) and node.name == 'MiciFccModal')
  namespace = {'rl': rl, 'gui_app': gui, 'HtmlRenderer': HtmlRenderer, 'NavRawScrollPanel': NavRawScrollPanel}
  exec(compile(ast.Module(body=[cls], type_ignores=[]), 'mici/device.py', 'exec'), namespace)
  return namespace['MiciFccModal'](str(root / 'openpilot/selfdrive/assets/offroad/mici_fcc.html'))

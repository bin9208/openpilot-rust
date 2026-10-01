"""Owned clock, subscription and modal-result seams for real product widget code."""

import ast
from pathlib import Path
import time
from types import SimpleNamespace, MethodType
from openpilot.cereal import car, log

KEYS = [
  'ExperimentalMode',
  'ExperimentalModeConfirmed',
  'OnroadCycleRequested',
  'LongitudinalPersonality',
  'IsMetric',
  'RecordAudio',
  'RecordFront',
  'OpenpilotEnabledToggle',
  'AlphaLongitudinalEnabled',
  'DevicePosition',
  'DoReboot',
  'DoShutdown',
  'DoUninstall',
  'LanguageSetting',
  'AdbEnabled',
  'SshEnabled',
  'JoystickDebugMode',
  'LongitudinalManeuverMode',
  'ShowDebugInfo',
  'GithubUsername',
  'GithubSshKeys',
  'UpdaterTargetBranch',
]


def car_bytes(scene):
  value = scene.get('car')
  if value is None:
    return None
  return car.CarParams.new_message(
    alphaLongitudinalAvailable=value['alpha_longitudinal_available'],
    openpilotLongitudinalControl=value['openpilot_longitudinal_control'],
    maxLateralAccel=value.get('max_lateral_accel', 0),
  ).to_bytes()


def attach_refresh(ui, scene):
  root = Path(__file__).resolve().parents[3]
  source = ast.parse((root / 'openpilot/selfdrive/ui/ui_state.py').read_text())
  cls = next(node for node in source.body if isinstance(node, ast.ClassDef) and node.name == 'UIState')
  method = next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == 'update_params')

  def decode(data, struct):
    with struct.from_bytes(data) as value:
      return value

  namespace = {
    'car': car,
    'messaging': SimpleNamespace(log_from_bytes=decode),
    'time': time,
    'active_usbgpu_compiled_path': lambda: 'fixture' if scene.get('models', {}).get('compiled') else None,
    'usbgpu_compile_pending': lambda: scene.get('models', {}).get('compile_pending', False),
  }
  exec(compile(ast.Module(body=[method], type_ignores=[]), 'ui_state.py', 'exec'), namespace)
  ui.update_params = MethodType(namespace['update_params'], ui)


class Effects:
  def __init__(self, scene, ui, gui, callbacks):
    self.scene = scene
    self.ui = ui
    self.gui = gui
    self.callbacks = callbacks
    self.effects = []
    self.dialogs = []
    self.offroad_callbacks = []
    if scene.get('capture_effects'):
      gui.push_widget = self.push
      gui.set_show_touches = lambda value: self.effects.append({'touches': value})
      gui.set_show_fps = lambda value: self.effects.append({'fps': value})

  def push(self, dialog):
    if hasattr(dialog, "options"):
      self.effects.append({"select": dialog.title, "options": dialog.options, "current": dialog.current})
      self.dialogs.append(dialog)
      return
    if hasattr(dialog, "owned_page"):
      self.effects.append({"page": dialog.owned_page})
      return
    if hasattr(dialog, '_min_text_size'):
      self.effects.append({'keyboard': dialog._title._text, 'text': dialog.text, 'minimum': dialog._min_text_size})
      self.dialogs.append(dialog)
      return
    if hasattr(dialog, '_minimum_length'):
      self.effects.append({'mici_input': dialog._hint_label._text, 'text': dialog._keyboard.text(), 'minimum': dialog._minimum_length})
      self.dialogs.append(dialog)
      return
    if hasattr(dialog, '_card'):
      self.effects.append({'mici_alert': dialog._card.get_text(), 'description': dialog._card.get_value()})
      return
    if hasattr(dialog, '_slider'):
      self.effects.append(
        {'mici_confirm': dialog._slider._label._text, 'exit': dialog._exit_on_confirm, 'red': type(dialog._slider).__name__ == 'RedBigSlider'}
      )
      self.dialogs.append(dialog)
      return
    if dialog._cancel_text == '':
      self.effects.append({'alert': dialog._label._text})
      return
    self.effects.append({'confirm': dialog._label._text, 'button': dialog._confirm_button._label._text, 'cancel': dialog._cancel_text, 'rich': dialog._rich})
    self.dialogs.append(dialog)

  def before(self, index, widget):
    step = next((step for step in self.scene.get('steps', []) if step['frame'] == index), {})
    if 'scroll' in step:
      widget._scroller.scroll_to(step['scroll'])
    for key, value in step.get('params', {}).items():
      if value is None:
        self.scene['params'].pop(key, None)
      else:
        self.scene['params'][key] = value
    if 'ignition' in step:
      self.ui.ignition = step['ignition']
    if 'started' in step:
      self.ui.started = step['started']
      for callback in self.offroad_callbacks:
        callback()
    if 'engaged' in step:
      self.ui.engaged = step['engaged']
      for callback in self.callbacks:
        callback()
    self.ui.sm.updated['selfdriveState'] = 'personality' in step
    if 'personality' in step:
      name = next(name for name, value in log.LongitudinalPersonality.schema.enumerants.items() if value == step['personality'])
      self.ui.sm['selfdriveState'] = SimpleNamespace(personality=name)
    if 'input_text' in step:
      dialog = self.dialogs.pop()
      if hasattr(dialog, '_minimum_length'):
        dialog._keyboard.set_text(step['input_text'])
        dialog._confirm_callback()
        dialog._dismiss_callback()
      else:
        from openpilot.system.ui.widgets import DialogResult

        dialog.set_text(step['input_text'])
        dialog._callback(DialogResult.CONFIRM)
    if step.get('flush_ssh'):
      fetcher = widget._ssh_keys.action_item._fetcher if self.scene['config']['big'] else widget._ssh_fetcher
      deadline = time.perf_counter() + 5
      while not fetcher._done:
        assert time.perf_counter() < deadline, 'owned SSH fixture did not finish'
        time.sleep(0.001)
    if 'selection' in step:
      self.dialogs[-1].selection = step['selection']
    if 'confirm' in step and self.dialogs:
      from openpilot.system.ui.widgets import DialogResult

      dialog = self.dialogs.pop()
      if hasattr(dialog, '_slider'):
        if step['confirm']:
          dialog._confirm_callback()
      else:
        dialog._callback(DialogResult.CONFIRM if step['confirm'] else DialogResult.CANCEL)
    return step

  def snapshot(self):
    return {
      'params': {key: self.scene.get('params', {}).get(key) for key in KEYS},
      'effects': list(self.effects),
      'personality': int(self.ui.personality),
      'raw_params': {key: bytes(value).hex() for key, value in self.scene.get('raw_params', {}).items()},
    }

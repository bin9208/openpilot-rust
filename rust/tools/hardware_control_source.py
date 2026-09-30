"""Run original hardware method bodies with filesystem and command fixture boundaries."""

import ast
from functools import cache, cached_property
from pathlib import Path
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
METHODS = {
  'booted',
  'reboot',
  'shutdown',
  'uninstall',
  'set_display_power',
  'set_screen_brightness',
  'set_power_save',
  'initialize_hardware',
  'has_internal_panda',
  'reset_internal_panda',
  'recover_internal_panda',
  'set_ir_power',
  'amplifier',
  'get_device_type',
}


def definitions(path, names, scope):
  tree = ast.parse((ROOT / path).read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names]
  for node in tree.body:
    if isinstance(node, ast.ClassDef):
      node.body = [member for member in node.body if isinstance(member, ast.FunctionDef) and member.name in METHODS]
  exec(compile(tree, path, 'exec'), scope)


def run(config):
  files = dict(config['files'])
  events = []
  faults = config.get('faults', {})

  def record(event):
    index = len(events)
    events.append(event)
    if str(index) in faults:
      raise OSError(faults[str(index)], __import__('os').strerror(faults[str(index)]), event.get('path', 'fixture'))

  class Open:
    def __init__(self, path, mode='r'):
      self.path, self.mode = str(path), mode

    def __enter__(self):
      return self

    def __exit__(self, *_args):
      return False

    def read(self):
      record({'kind': 'read', 'path': self.path})
      if self.path not in files:
        raise FileNotFoundError(2, 'No such file or directory', self.path)
      return files[self.path]

    def readlines(self):
      return self.read().splitlines(keepends=True)

    def write(self, value):
      if isinstance(value, bytes):
        value = value.decode()
      record({'kind': 'write', 'path': self.path, 'value': value})
      files[self.path] = value

  def command(argv, *, shell=False, encoding=None):
    if shell:
      argv = ['/bin/sh', '-c', argv]
    record({'kind': 'output', 'argv': argv})
    status = config.get('command_status', 0)
    if status:
      raise subprocess.CalledProcessError(status, argv)
    text = config.get('command_text', '')
    return text if encoding else text.encode()

  def call(argv):
    record({'kind': 'call', 'argv': argv})
    return config.get('command_status', 0)

  def system(text):
    record({'kind': 'shell', 'text': text})
    return config.get('command_status', 0)

  def lite():
    record({'kind': 'lite'})
    return config.get('lite', False)

  def monotonic():
    record({'kind': 'monotonic'})
    return config.get('now', 0)

  class ResetPath:
    def __init__(self, path):
      self.path = path

    def touch(self):
      record({'kind': 'touch', 'path': self.path})
      files.setdefault(self.path, '')

  class Amplifier:
    def set_global_shutdown(self, amp_disabled):
      record({'kind': 'amp_shutdown', 'disabled': amp_disabled})
      return config.get('amplifier_result', True)

    def initialize_configuration(self, model):
      record({'kind': 'amp_initialize', 'model': model})
      return config.get('amplifier_result', True)

  scope = {
    'open': Open,
    'print': lambda value: record({'kind': 'print', 'value': value}),
    'os': SimpleNamespace(system=system, sync=lambda: record({'kind': 'sync'})),
    'subprocess': SimpleNamespace(check_output=command, call=call, CalledProcessError=subprocess.CalledProcessError),
    'time': SimpleNamespace(sleep=lambda seconds: record({'kind': 'sleep', 'seconds': seconds}), monotonic=monotonic),
    'cached_property': cached_property,
    'cache': cache,
    'ABC': __import__('abc').ABC,
    'abstractmethod': lambda value: value,
    'get_device_type': lambda: config['model'],
    'is_c3x_lite': lite,
    'Amplifier': Amplifier,
    'Path': ResetPath,
    'GPIO': SimpleNamespace(SOM_ST_IO=49, STM_RST_N=124, STM_BOOT0=134),
  }
  definitions('openpilot/common/utils.py', {'sudo_write', 'sudo_read'}, scope)
  definitions('openpilot/common/gpio.py', {'gpio_init', 'gpio_set', 'get_irq_action', 'get_irqs_for_action'}, scope)
  definitions('openpilot/system/hardware/base.py', {'HardwareBase'}, scope)
  definitions('openpilot/system/hardware/tici/hardware.py', {'Tici', 'affine_irq'}, scope)
  hardware = scope['HardwareBase']() if config['model'] == 'pc' else scope['Tici']()
  outcomes = []
  for operation in config['operations']:
    try:
      match operation['kind']:
        case 'initialize':
          value = hardware.initialize_hardware()
        case 'power_save':
          value = hardware.set_power_save(operation['enabled'])
        case 'display':
          value = hardware.set_display_power(operation['on'])
        case 'brightness':
          value = hardware.set_screen_brightness(operation['percent'])
        case 'ir':
          value = hardware.set_ir_power(operation['percent'])
        case 'reset':
          value = hardware.reset_internal_panda()
        case 'recover':
          value = hardware.recover_internal_panda()
        case 'booted':
          value = hardware.booted()
        case 'reboot':
          value = hardware.reboot()
        case 'shutdown':
          value = hardware.shutdown()
        case 'uninstall':
          value = hardware.uninstall()
        case 'has_panda':
          value = hardware.has_internal_panda()
        case 'affine':
          value = scope['affine_irq'](operation['core'], operation['action'])
        case 'sudo_write':
          value = scope['sudo_write'](operation['value'], operation['path'])
        case _:
          raise AssertionError(operation)
      outcomes.append({'value': value})
    except OSError as error:
      outcomes.append({'error': 'io', 'errno': error.errno})
    except subprocess.CalledProcessError as error:
      outcomes.append({'error': 'command', 'status': error.returncode})
    except (ValueError, UnicodeError):
      outcomes.append({'error': 'other'})
  return {'events': events, 'outcomes': outcomes, 'files': files}

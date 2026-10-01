"""Unmodified Python hardware worker bodies behind owned clock/hardware/Params boundaries."""

import ast
from collections import OrderedDict, namedtuple
import io
from numbers import Number
from pathlib import Path
import queue
import threading
from types import SimpleNamespace

import numpy as np
from openpilot.cereal import car, log

ROOT = Path(__file__).resolve().parents[2]


def load(path, scope):
  tree = ast.parse((ROOT / path).read_text())
  tree.body = [
    node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom)) and not (isinstance(node, ast.If) and isinstance(node.test, ast.Compare))
  ]
  exec(compile(tree, path, 'exec'), scope)


def run(case):
  current = {'now': 0.0, 'power': 0.0, 'step': {}}
  values = {'UptimeOffroad': 0.0, 'UptimeOnroad': 0.0, 'CarBatteryCapacity': case.get('capacity', 0), 'MaxTimeOffroadMin': 1800}
  saved, errors = [], []

  class Params:
    def get(self, key, **kwargs):
      return values.get(key)

    def get_bool(self, key):
      return bool(values.get(key, False))

    def get_int(self, key):
      return int(values.get(key, 0))

    def put(self, key, value):
      values[key] = value

    put_bool = put
    put_bool_nonblocking = put

    def put_nonblocking(self, key, value):
      saved.append(value)
      self.put(key, value)

    def remove(self, key):
      values.pop(key, None)

  cloudlog = SimpleNamespace(
    exception=lambda text: errors.append(text), event=lambda *args, **kwargs: None, error=lambda *args: None, warning=lambda *args: None
  )
  hardware = SimpleNamespace(get_current_power_draw=lambda: current['power'])
  scope = {
    'np': np,
    'Number': Number,
    'Params': Params,
    'HARDWARE': hardware,
    'time': SimpleNamespace(monotonic=lambda: current['now']),
    'threading': threading,
    'cloudlog': cloudlog,
    'statlog': SimpleNamespace(gauge=lambda *args: None, sample=lambda *args: None),
  }
  load('openpilot/common/pid.py', scope)
  load('openpilot/system/hardware/fan_controller.py', scope)
  load('openpilot/system/hardware/power_monitoring.py', scope)
  match case['kind']:
    case 'fan':
      fan = scope['FanController'](2, case['device'])
      return [fan.update(temp, ignition) for temp, ignition in case['steps']]
    case 'power':
      power = scope['PowerMonitoring']()
      results = []
      for step in case['steps']:
        current.update(now=step['now'], power=step['power'])
        saved.clear()
        errors.clear()
        power.calculate(step['voltage'], step['ignition'])
        shutdown = step['shutdown']
        current['now'] = shutdown['now']
        values.update(MaxTimeOffroadMin=shutdown['max_offroad_minutes'], DisablePowerDown=shutdown['disable'], ForcePowerDown=shutdown['force'])
        result = power.should_shutdown(shutdown['ignition'], shutdown['in_car'], shutdown['off_ts'], shutdown['started_seen'])
        results.append(
          {
            'state': {
              'last_measurement': power.last_measurement_time,
              'last_save': power.last_save_time,
              'used': power.power_used_uWh,
              'capacity': power.car_battery_capacity_uWh,
              'voltage': power.car_voltage_mV,
              'instant_voltage': power.car_voltage_instant_mV,
            },
            'save': saved[-1] if saved else None,
            'error': errors[-1] if errors else None,
            'shutdown': result,
          }
        )
      return results
    case 'policy':
      return policy(case, current, values, scope, Params, hardware)
    case _:
      raise ValueError('unknown fixture kind')


def policy(case, current, values, scope, params_type, hardware):
  results = []
  steps = iter(case['steps'])
  done = threading.Event()
  values.update(HasAcceptedTerms='2', CompletedTrainingVersion='0.2.0')
  power_save = [False]

  def status(field):
    return current['step'].get('startup', {}).get(field, True)

  class SubMaster:
    def __init__(self, *_args, **_kwargs):
      self.frame = -1
      self.updated = {'pandaStates': False, 'selfdriveState': False}
      self.recv_time = {'pandaStates': 0}
      self.alive = {'gpsLocationExternal': False}
      self.data = {}

    def update(self, timeout):
      assert timeout == 150
      step = next(steps)
      current.update(now=step['now'], step=step)
      self.frame = step['frame']
      self.updated['pandaStates'] = step.get('panda_updated', False)
      self.recv_time['pandaStates'] = step.get('panda_receive_time', 0.0)
      panda = log.PandaState.new_message(
        pandaType='dos', ignitionLine=step.get('ignition', False), harnessStatus='normal' if step.get('in_car', False) else 'notConnected'
      )
      self.data['pandaStates'] = [panda] if step.get('panda_present', False) else []
      self.data['peripheralState'] = log.PeripheralState.new_message()
      values.update(
        OnroadCycleRequested=step.get('cycle_requested', False),
        DoUninstall=not status('not_uninstalling'),
        HasAcceptedTerms='2' if status('accepted_terms') else 'wrong',
        CompletedTrainingVersion='0.2.0' if status('completed_training') else 'wrong',
        IsDriverViewEnabled=not status('not_driver_view'),
        IsTakingSnapshot=not status('not_taking_snapshot'),
      )
      for key, field in [('Offroad_ConnectivityNeeded', 'up_to_date'), ('Offroad_ExcessiveActuation', 'no_excessive_actuation')]:
        if status(field):
          values.pop(key, None)
        else:
          values[key] = {'text': 'fixture'}
      cp = car.CarParams.new_message(brand='tesla' if step.get('tesla', False) else 'hyundai')
      values['CarParams'] = cp.to_bytes()
      if step is case['steps'][-1]:
        done.set()

    def __getitem__(self, key):
      return self.data[key]

  def publish(_service, message):
    value = message.deviceState.to_dict()
    results.append(
      {key: value[key] for key in ['started', 'startedMonoTime', 'thermalStatus', 'maxTempC', 'fanSpeedPercentDesired']}
      | {'power_save': power_save[0], 'temperature_alert': 'Offroad_TemperatureTooHigh' in values}
    )
    results[-1]['thermalStatus'] = ['green', 'yellow', 'red', 'danger'].index(results[-1]['thermalStatus'])

  def new_message(_service, valid):
    event = log.Event.new_message(logMonoTime=int(current['now'] * 1e9), valid=valid)
    event.init('deviceState')
    return event

  def thermal_message():
    step = current['step']
    return log.DeviceState.new_message(
      memoryTempC=step.get('offroad_temperature', 0.0), cpuTempC=[], gpuTempC=[], pmicTempC=[step.get('pmic_temperature', 0.0)]
    )

  def set_alert(key, visible, extra_text=None):
    if visible:
      values[key] = {'extra': extra_text}
    else:
      values.pop(key, None)

  hardware.get_device_type = lambda: case['device']
  hardware.get_thermal_config = lambda: SimpleNamespace(get_msg=thermal_message)
  hardware.initialize_hardware = lambda: None
  hardware.get_gpu_usage_percent = lambda: 0
  hardware.get_screen_brightness = lambda: current['step'].get('brightness', 0)
  hardware.booted = lambda: current['step'].get('booted', False)
  hardware.get_som_power_draw = lambda: 0
  hardware.set_power_save = lambda value: power_save.__setitem__(0, value)
  scope.update(
    OrderedDict=OrderedDict,
    namedtuple=namedtuple,
    log=log,
    car=car,
    DT_HW=0.5,
    queue=queue,
    SERVICE_LIST={'pandaStates': SimpleNamespace(frequency=10)},
    TICI=case['device'] != 'pc',
    AGNOS=False,
    PC=case['device'] == 'pc',
    messaging=SimpleNamespace(
      PubMaster=lambda _services: SimpleNamespace(send=publish),
      SubMaster=SubMaster,
      new_message=new_message,
      log_from_bytes=lambda value, schema: schema.from_bytes(value).__enter__(),
    ),
    psutil=SimpleNamespace(virtual_memory=lambda: SimpleNamespace(percent=0), cpu_percent=lambda **kwargs: []),
    get_available_percent=lambda default: 100 if status('free_space') else 2,
    set_offroad_alert=set_alert,
    gpio_set=lambda *args: None,
    GPIO=SimpleNamespace(SOM_ST_IO=49),
    terms_version='2',
    training_version='0.2.0',
    strip_deprecated_keys=lambda value: value,
    open=lambda *args: io.StringIO(),
  )
  load('openpilot/common/filter_simple.py', scope)
  load('openpilot/system/hardware/hardwared.py', scope)
  scope['hardware_thread'](done, queue.Queue(maxsize=1))
  return results

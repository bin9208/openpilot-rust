from pathlib import Path
from typing import TypeAlias
from contextlib import redirect_stdout
import io
from can_source import ROOT, load

Json: TypeAlias = str | int | float | bool | None | list['Json'] | dict[str, 'Json']


class Settings:
  def __init__(self, nnff: bool, disable_min: bool):
    self.values = {'NNFF': nnff, 'DisableMinSteerSpeed': disable_min}
    self.model: str | None = None

  def get_bool(self, key: str) -> bool:
    return self.values[key]

  def put_nonblocking(self, key: str, value: str) -> None:
    assert key == 'NNFFModelName'
    self.model = value


def normalize(value: Json) -> Json:
  import math
  match value:
    case float() if not math.isfinite(value):
      return 'nan' if math.isnan(value) else str(value)
    case dict():
      return {key: normalize(item) for key, item in value.items()}
    case list():
      return [normalize(item) for item in value]
    case _:
      return value


def trace(case: dict) -> Json:
  load()
  from opendbc.car import structs, interfaces
  from opendbc.car.interfaces import CarInterfaceBase, CarStateBase
  if case['op'] == 'params':
    settings = Settings(case['nnff'], case['disable_min'])
    interfaces.Params = lambda: settings
    class Interface(CarInterfaceBase):
      @staticmethod
      def _get_params(ret, candidate, fingerprint, car_fw, alpha_long, is_release, docs):
        ret.notCar = case['not_car']
        if case['angle']:
          ret.steerControlType = structs.CarParams.SteerControlType.angle
        if case['torque'] is not None:
          options = case['torque']
          CarInterfaceBase.configure_torque_tune(candidate, ret.lateralTuning, options['deadzone_deg'], options['use_steering_angle'])
        return ret
    firmware = [structs.CarParams.CarFw.new_message(ecu=fw['ecu'], fwVersion=bytes(fw['fw_version'])) for fw in case['firmware']]
    try:
      printed = io.StringIO()
      with redirect_stdout(printed):
        cp = Interface.get_params(case['candidate'], {}, firmware, False, True, False)
    except KeyError:
      error = 'missing_torque' if case['candidate'] in interfaces.PLATFORMS else 'unknown_platform'
      return {'error': error}
    return normalize({'params': cp.to_dict(), 'model': settings.model, 'prints': printed.getvalue().splitlines()})
  class State(CarStateBase):
    def update(self, can_parsers):
      raise RuntimeError('state fixture never calls vehicle update')
  state = State(structs.CarParams.new_message())
  result = []
  for step in case['steps']:
    match step['op']:
      case 'speed':
        value = list(state.update_speed_kf(step['value']))
      case 'lamp':
        value = list(state.update_blinker_from_lamp(step['time'], step['left'], step['right']))
      case 'stalk':
        value = list(state.update_blinker_from_stalk(step['time'], step['left'], step['right']))
      case 'pressed':
        value = state.update_steering_pressed(step['pressed'], step['minimum'])
      case 'gear':
        value = state.parse_gear_shifter(step['value'])
      case 'wheels':
        state.CP.wheelSpeedFactor = step['factor']
        wheels = state.get_wheel_speeds(*step['values'], step['unit'])
        value = [wheels.fl, wheels.fr, wheels.rl, wheels.rr]
      case 'buttons':
        state.CP.pcmCruise = step['pcm']
        events = [structs.CarState.ButtonEvent.new_message(type=kind, pressed=pressed) for kind, pressed in step['events']]
        value = state.update_button_enable(events)
      case _:
        raise AssertionError(f'unknown owned fixture operation: {step}')
    result.append(value)
  return normalize(result)


def decode(value: Json) -> Json:
  from opendbc.car import structs
  if isinstance(value, dict) and 'wire' in value:
    with structs.CarParams.from_bytes(bytes(value['wire'])) as cp:
      return normalize({'params': cp.to_dict(), 'model': value['model'], 'prints': value['prints']})
  return value

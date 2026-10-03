import argparse
import ast
import hashlib
import io
import itertools
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
from typing import Any

from can_source import ROOT, load
from card_vehicle_source import normalize
from check_card_vehicle import compare

KEYS = ('CarName', 'FingerPrints', 'FirmwareQueryDone', 'SecOCKey', 'CarParamsPrevRoute',
        'CarParams', 'CarParamsCache', 'CarParamsPersistent', 'DisengageOnAccelerator', 'OpenpilotEnabledToggle')
CP_KEYS = ('CarParams', 'CarParamsCache', 'CarParamsPersistent')


class Settings:
  def __init__(self, values: dict[str, list[int]]) -> None:
    self.values = {key: bytes(value) for key, value in values.items()}

  def get(self, key: str) -> str | bytes | None:
    value = self.values.get(key)
    if value == b'':
      return None
    return value.decode() if key == 'SecOCKey' and value is not None else value

  def get_bool(self, key: str) -> bool:
    return self.get(key) == b'1'

  def put(self, key: str, value: str | bytes) -> None:
    self.values[key] = value.encode() if isinstance(value, str) else value

  put_nonblocking = put


def cases() -> list[dict[str, Any]]:
  load()
  from openpilot.cereal import car
  result = []
  for enabled, controller, dashcam, disengage, secoc in itertools.product((False, True), repeat=5):
    for saved, user in ((None, None), ('', None), ('00112233445566778899aabbccddeeff', None),
                        ('0011', None), ('badkey', None), ('00 11 22 33 44 55 66 77 88 99 aa bb cc dd ee ff', None),
                        ('00\t11', None), ('001', None), (None, '00112233445566778899aabbccddeeff'),
                        ('badkey', '00112233445566778899aabbccddeeff'), (None, 'too short')):
      cp = car.CarParams.new_message(brand='body', carFingerprint='COMMA_BODY', dashcamOnly=dashcam,
           safetyConfigs=[dict(safetyModel='body')], secOcRequired=secoc, openpilotLongitudinalControl=True)
      identification = dict(candidate='COMMA_BODY', observed=[[0, [[123, 8], [456, 64]]], [4, [[100, 8]]]],
          vin='1HGCM82633A004352', firmware=[], source='fixed', exact_match=not dashcam, cached=False,
          vin_rx_address=None, vin_rx_bus=None, ecu_responses=[], fw_query_time=0., packets=202)
      settings = {'OpenpilotEnabledToggle': list(b'1' if enabled else b'0'),
                  'DisengageOnAccelerator': list(b'1' if disengage else b'0'), 'CarParamsPersistent': list(b'previous route')}
      if saved is not None:
        settings['SecOCKey'] = list(saved.encode())
      result.append(dict(params=list(cp.to_bytes()), identification=identification, settings=settings,
                         controller=controller, user_key=user))
  for previous in (None, []):
    case = result[0].copy()
    case['settings'] = case['settings'].copy()
    if previous is None:
      del case['settings']['CarParamsPersistent']
    else:
      case['settings']['CarParamsPersistent'] = previous
    result.append(case)
  return result


def source_prepare(case: dict[str, Any]) -> dict[str, Any]:
  from openpilot.cereal import car
  from openpilot.selfdrive.car.alternative_experience import get_alternative_experience
  settings = Settings(case['settings'])
  with car.CarParams.from_bytes(bytes(case['params'])) as reader:
    cp = reader.as_builder()
  identification = case['identification']
  cp.carVin = identification['vin']
  cp.carFw = []
  cp.fingerprintSource = identification['source']
  cp.fuzzyFingerprint = not identification['exact_match']
  settings.put('CarName', identification['candidate'])
  settings.put('FingerPrints', repr({bus: dict(entries) for bus, entries in identification['observed']}))
  settings.put('FirmwareQueryDone', b'1')
  vehicle = SimpleNamespace(CC=SimpleNamespace() if case['controller'] else None, CS=SimpleNamespace())
  runtime = SimpleNamespace(CP=cp, CI=vehicle, params=settings)
  warnings = []
  warning_states = []
  def warning(message):
    warnings.append(message)
    warning_states.append({key: list(value) if (value := settings.values.get(key)) is not None else None
                           for key in ('CarParamsPrevRoute', 'CarParams')})
  path = ROOT / 'openpilot/selfdrive/car/card.py'
  tree = ast.parse(path.read_text())
  node = next(item for item in tree.body if isinstance(item, ast.ClassDef) and item.name == 'Car')
  constructor = next(item for item in node.body if isinstance(item, ast.FunctionDef) and item.name == '__init__')
  start = next(index for index, item in enumerate(constructor.body)
               if isinstance(item, ast.Assign) and ast.unparse(item.targets[0]) == 'self.CP.alternativeExperience')
  end = next(index for index, item in enumerate(constructor.body)
             if isinstance(item, ast.Assign) and ast.unparse(item.targets[0]) == 'self.mock_carstate')
  constructor.body = constructor.body[start:end]
  constructor.decorator_list = []
  scope: dict[str, Any] = dict(car=car, structs=car, get_alternative_experience=get_alternative_experience,
      cloudlog=SimpleNamespace(warning=warning),
      open=lambda path: io.StringIO(case['user_key']) if case['user_key'] is not None else (_ for _ in ()).throw(FileNotFoundError(path)))
  exec(compile(ast.Module(body=[constructor], type_ignores=[]), str(path), 'exec'), scope)
  try:
    scope['__init__'](runtime)
    output = dict(params=list(settings.values['CarParams']), key=list(vehicle.CS.secoc_key) if hasattr(vehicle.CS, 'secoc_key') else None,
                  controller=case['controller'] and settings.get_bool('OpenpilotEnabledToggle') and not cp.dashcamOnly, error=False)
  except ValueError:
    output = dict(error=True)
  return dict(output=output, warnings=warnings, warning_states=warning_states, settings={key: list(value) if (value := settings.values.get(key)) is not None else None for key in KEYS})


def decoded(value: dict[str, Any]) -> dict[str, Any]:
  from openpilot.cereal import car
  if not value['output']['error']:
    with car.CarParams.from_bytes(bytes(value['output']['params'])) as cp:
      value['output']['params'] = normalize(cp.to_dict())
  for key in CP_KEYS:
    raw = value['settings'][key]
    if raw is not None and raw != list(b'previous route'):
      with car.CarParams.from_bytes(bytes(raw)) as cp:
        value['settings'][key] = normalize(cp.to_dict())
  return value


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = cases()
  expected = [source_prepare(case) for case in request]
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  target = args.evidence / 'native.json'
  command = [str(args.binary.resolve()), str(target.resolve())]
  child = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(json.dumps(command) + '\n' + child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  left = [decoded(value) for value in expected]
  right = [decoded(value) for value in json.loads(target.read_text())]
  (args.evidence / 'source-decoded.json').write_text(json.dumps(left) + '\n')
  (args.evidence / 'native-decoded.json').write_text(json.dumps(right) + '\n')
  try:
    compare(left, right)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), errors=sum(value['output']['error'] for value in expected),
      runtime_python=False, observable='full prepared CarParams, exact SecOC warnings, acceptance/failure and Params writes for active/passive/controller/dashcam routes',
      scope='unchanged startup constructor segment with owned Params/key-file boundary; constructor and native sockets tested separately',
      command=command, binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
      source_sha256={'openpilot/selfdrive/car/card.py': hashlib.sha256((ROOT / 'openpilot/selfdrive/car/card.py').read_bytes()).hexdigest()})
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

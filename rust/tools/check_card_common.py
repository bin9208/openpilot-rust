import argparse
import ast
from contextlib import redirect_stdout
import hashlib
import io
import itertools
import json
from pathlib import Path
import subprocess

from can_source import ROOT, load
from card_vehicle_source import normalize
from check_card_vehicle import compare


class Settings:
  def __init__(self, nnff: bool, lite: bool) -> None:
    self.values = {'NNFF': nnff, 'NNFFLite': lite}
    self.maximum = None

  def put_int(self, key: str, value: int) -> None:
    assert key == 'LongitudinalPersonalityMax'
    self.maximum = list(str(value).encode())

  def get_bool(self, key: str) -> bool:
    return self.values[key]


def cases(evidence: Path) -> list[dict]:
  load()
  from openpilot.cereal import car
  catalog = json.loads((ROOT / 'rust/crates/card/data/vehicle.json').read_text())
  result = []
  for index, platform in enumerate(catalog['platforms']):
    cp = car.CarParams.new_message(carFingerprint=platform['candidate'])
    if index % 3 == 0:
      cp.carFw = [dict(ecu='eps', fwVersion=b"\x00\xff'\"\\eps\n")]
    wire = list(cp.to_bytes())
    for nnff, lite in itertools.product((False, True), repeat=2):
      result.append(dict(params=wire, nnff=nnff, lite=lite, assets=None,
                         inputs=[[10., 0., .2], [0., -.2], [2., 3., 4.]]))
  for name, weights, model in (
      ('missing', None, None), ('bad-weights', '{', None), ('empty-models', '{}', None),
      ('bad-model', '{}', '{'), ('no-output-size', '{}', '{"input_size": 1}')):
    path = evidence.resolve() / name
    path.mkdir(parents=True)
    if weights is not None:
      (path / 'neural_ff_weights.json').write_text(weights)
      (path / 'lat_models').mkdir()
    if model is not None:
      (path / 'lat_models/COMMA_BODY.json').write_text(model)
    cp = car.CarParams.new_message(carFingerprint='COMMA_BODY')
    result.append(dict(params=list(cp.to_bytes()), nnff=False, lite=False, assets=str(path), inputs=[]))
  return result


def trace(case: dict) -> dict:
  from openpilot.cereal import car
  from opendbc.car import interfaces
  settings = Settings(case['nnff'], case['lite'])
  assets = Path(case['assets']) if case['assets'] is not None else ROOT / 'opendbc_repo/opendbc/car/torque_data'
  path = ROOT / 'opendbc_repo/opendbc/car/interfaces.py'
  tree = ast.parse(path.read_text())
  node = next(item for item in tree.body if isinstance(item, ast.ClassDef) and item.name == 'CarInterfaceBase')
  node.bases = []
  node.body = [item for item in node.body if isinstance(item, ast.FunctionDef)
               and item.name in ('__init__', 'check_comma_nn_ff_support', 'initialize_lat_torque_nn')]
  constructor = next(item for item in node.body if item.name == '__init__')
  start = next(index for index, item in enumerate(constructor.body) if isinstance(item, ast.Expr)
               and ast.unparse(item.value).startswith("Params().put_int('LongitudinalPersonalityMax'"))
  constructor.body = constructor.body[start:]
  constructor.args.args[1].annotation = None
  selected = []
  original = interfaces.get_nn_model_path

  def model_path(candidate: str, firmware: str):
    value = original(candidate, firmware)
    selected.append(value)
    return value

  interfaces.get_nn_model_path = model_path
  previous_directory = interfaces.TORQUE_NN_MODEL_PATH
  interfaces.TORQUE_NN_MODEL_PATH = str(assets / 'lat_models')
  scope = dict(Params=lambda: settings, NEURAL_PARAMS_PATH=str(assets / 'neural_ff_weights.json'),
               json=json, get_nn_model=interfaces.get_nn_model)
  exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), 'exec'), scope)
  try:
    with car.CarParams.from_bytes(bytes(case['params'])) as cp:
      runtime = scope['CarInterfaceBase'].__new__(scope['CarInterfaceBase'])
      runtime.CP = cp
      runtime.__init__(cp)
      model = runtime.lat_torque_nn_model
      result = dict(use_nnff=runtime.use_nnff, use_nnff_lite=runtime.use_nnff_lite,
          file=Path(selected[-1]).name if selected[-1] is not None else None,
          friction=model.friction_override if model is not None else None,
          values=[model.evaluate(value) if model is not None else None for value in case['inputs']],
          maximum=settings.maximum, error=False)
  except (OSError, ValueError, KeyError, TypeError, AssertionError, IndexError):
    result = dict(maximum=settings.maximum, error=True)
  finally:
    interfaces.get_nn_model_path = original
    interfaces.TORQUE_NN_MODEL_PATH = previous_directory
  return normalize(result)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = cases(args.evidence)
  source_log = io.StringIO()
  with redirect_stdout(source_log):
    expected = [trace(case) for case in request]
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(source_log.getvalue())
  target = args.evidence / 'native.json'
  command = [str(args.binary.resolve()), str(target.resolve()), str(ROOT / 'opendbc_repo/opendbc/car/torque_data'), str(args.numerics)]
  child = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(json.dumps(command) + '\n' + child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  try:
    compare(expected, actual)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), errors=sum(value['error'] for value in expected),
      selected=sum(value.get('file') is not None for value in expected), runtime_python=False,
      observable='original constructor numerical asset selection/validation, EPS repr, NNFF/Lite flags, personality maximum and exact Flux values',
      scope='unchanged common constructor statements and model implementation; CAN/brand construction tested separately',
      binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
      source_sha256={str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
         for path in [ROOT / 'opendbc_repo/opendbc/car/interfaces.py', *sorted((ROOT / 'opendbc_repo/opendbc/car/torque_data').rglob('*.json'))]},
      numerics_manifest=json.loads((args.numerics / 'manifest.json').read_text()), command=command)
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

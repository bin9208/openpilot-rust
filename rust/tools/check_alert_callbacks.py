import argparse
import ast
import copy
from enum import IntFlag
import hashlib
import json
import logging
import math
import os
from pathlib import Path
import re
import subprocess
from types import SimpleNamespace as NS

import capnp

from generate_selfdrive_alerts import ROOT, generate, load_source, serialize_alert
from openpilot.cereal import log
from openpilot.common.constants import CV


def source_translator(language):
  path = ROOT / 'openpilot/system/ui/lib/multilang.py'
  nodes = [node for node in ast.parse(path.read_text()).body if isinstance(node, (ast.ClassDef, ast.FunctionDef)) or (
    isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in ('PLURAL_SELECTORS', 'UNIFONT_LANGUAGES') for target in node.targets))]
  directory = ROOT / 'openpilot/selfdrive/ui/translations'
  namespace = {'Params': None, 'json': json, 're': re, 'cloudlog': logging.getLogger('translation-oracle'),
               'LANGUAGES_FILE': directory / 'languages.json', 'TRANSLATIONS_DIR': directory}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  translator = namespace['Multilang']()
  translator._language = language
  translator.setup()
  return translator


def constant(path, name, namespace=None):
  tree = ast.parse((ROOT / path).read_text())
  node = next(node for node in tree.body if isinstance(node, ast.Assign)
              and any(isinstance(target, ast.Name) and target.id == name for target in node.targets))
  return eval(compile(ast.Expression(node.value), str(ROOT / path), 'eval'), namespace or {})


def setup_source(mici, language):
  source = load_source('mici' if mici else 'tici')
  translator = source_translator(language)
  source.update(tr=translator.tr, trn=translator.trn, CV=CV,
                MIN_SPEED_FILTER=constant('openpilot/selfdrive/locationd/calibrationd.py', 'MIN_SPEED_FILTER', {'CV': CV}),
                SAMPLE_RATE=constant('openpilot/system/micd.py', 'SAMPLE_RATE'),
                SAMPLE_BUFFER=constant('openpilot/system/micd.py', 'SAMPLE_BUFFER'),
                FEEDBACK_MAX_DURATION=constant('openpilot/selfdrive/ui/feedback/feedbackd.py', 'FEEDBACK_MAX_DURATION'))
  path = ROOT / 'opendbc_repo/opendbc/car/hyundai/values.py'
  flags = next(node for node in ast.parse(path.read_text()).body if isinstance(node, ast.ClassDef) and node.name == 'HyundaiFlags')
  source['IntFlag'] = IntFlag
  exec(compile(ast.Module(body=[flags], type_ignores=[]), str(path), 'exec'), source)
  return source


class Params:
  def __init__(self, values):
    self.values = values
    self.reads = []

  def get(self, key):
    self.reads.append(['get', key])
    return self.values.get(key)

  def get_int(self, key):
    self.reads.append(['get_int', key])
    return self.values[key]

  def get_bool(self, key):
    self.reads.append(['get_bool', key])
    return self.values[key]


class Messages(dict):
  def __init__(self, snapshot):
    s = snapshot
    status = log.LiveCalibrationData.Status
    super().__init__(
      liveCalibration=NS(calStatus=status.recalibrating if s['calibration_recalibrating'] else status.uncalibrated,
                         calPerc=s['calibration_percent'], rpyCalib=s['calibration_rpy']),
      audioFeedback=NS(blockNum=s['feedback_block']),
      deviceState=NS(freeSpacePercent=s['free_space_percent'], cpuTempC=s['cpu_temps'], gpuTempC=s['gpu_temps'],
                     memoryTempC=s['memory_temp'], memoryUsagePercent=s['memory_usage_percent']),
      modelV2=NS(velocity=NS(x=s['model_velocity']), frameDropPerc=s['frame_drop_percent']),
      managerState=NS(processes=[NS(name=p['name'], running=p['running'], shouldBeRunning=p['should_be_running']) for p in s['processes']]),
      liveParameters=NS(angleOffsetValid=s['angle_offset_valid'], angleOffsetDeg=s['angle_offset'], steerRatioValid=s['steer_ratio_valid'],
                        steerRatio=s['steer_ratio'], stiffnessFactorValid=s['stiffness_factor_valid'], stiffnessFactor=s['stiffness_factor']),
      carControl=NS(actuators=NS(accel=s['accel'], torque=s['torque'])), alertDebug=NS(alertText1=s['debug_text_1'], alertText2=s['debug_text_2']))
    self.data = {item['service']: None for item in s['health']}
    self.checks = {item['service']: item['all_checks'] for item in s['health']}

  def all_checks(self, services):
    return all(self.checks[service] for service in services)


def original(row, source):
  s = copy.deepcopy(row['snapshot'])
  if 'nonfinite' in row:
    field = row['nonfinite']['field']
    value = {'nan': math.nan, 'inf': math.inf, 'neg_inf': -math.inf}[row['nonfinite']['value']]
    if field in ('cpu_first', 'cpu_last', 'gpu_first'):
      s['gpu_temps' if field == 'gpu_first' else 'cpu_temps'] = [50., value] if field == 'cpu_last' else [value, 50.]
    elif field in ('calibration_pitch', 'calibration_yaw'):
      s['calibration_rpy'] = [0., value, 0.] if field == 'calibration_pitch' else [0., 0., value]
    elif field == 'model_velocity':
      s[field] = [value]
    else:
      s[field] = value
  cp = NS(brand=s['brand'], flags=s['flags'], minEnableSpeed=s['min_enable_speed'], minSteerSpeed=s['min_steer_speed'])
  params = Params(row['params'])
  source['Params'] = lambda: params
  source['get_short_branch'] = lambda: row['branch']
  previous = os.environ.pop('REPLAY', None)
  if row['replay']:
    os.environ['REPLAY'] = '1'
  try:
    callback = source[row['callback']['name']]
    if 'text' in row['callback']:
      callback = callback(row['callback']['text'])
    personality = ('aggressive', 'standard', 'relaxed', 'moreRelaxed')[row['personality']]
    try:
      alert = callback(cp, NS(vEgo=s['ego_speed']), Messages(s), row['metric'], row['soft_disable_time'], personality)
    except (ValueError, OverflowError) as error:
      assert 'nonfinite' in row, error
      return {'error': 'cannot round nonfinite alert value to an integer', 'reads': params.reads,
              'source_exception': {'class': type(error).__name__, 'message': str(error)}}
    if row.get('wire_check'):
      assert alert.alert_text_2 is None
      message = log.Event.new_message()
      state = message.init('selfdriveState')
      try:
        state.alertText2 = source['tr'](alert.alert_text_2)
      except capnp.KjException as error:
        return {'error': 'missing alert text parameter NNFFModelName', 'reads': params.reads,
                'source_exception': {'class': type(error).__name__, 'message': str(error),
                                     'phase': 'original wire construction; native callback resolution'}}
      raise AssertionError('source unexpectedly accepted absent alert text')
    return {'alert': serialize_alert(alert), 'reads': params.reads}
  finally:
    os.environ.pop('REPLAY', None)
    if previous is not None:
      os.environ['REPLAY'] = previous


def rows():
  definitions = generate()['tici']
  callbacks = {json.dumps(category['definition']['callback'], sort_keys=True): category['definition']['callback']
               for event in definitions for category in event['definitions'] if category['definition']['kind'] == 'callback'}
  languages = list(json.loads((ROOT / 'openpilot/selfdrive/ui/translations/languages.json').read_text()).values())
  result = []
  for mici in (False, True):
    for language in languages:
      for index in range(12):
        s = {
          'brand': ('hyundai', 'honda', 'tesla', 'mazda', 'nissan', 'toyota')[index % 6], 'flags': (0, 8, 16)[index % 3],
          'min_enable_speed': (-10., -0., 0., .125, 1.25, 8.9408, 40., 120., .5, 1.5, 2.5, 3.5)[index],
          'min_steer_speed': float(index) / 3.6 + .5 / 3.6, 'ego_speed': index * .25,
          'model_velocity': [] if index == 0 else [1.25], 'calibration_recalibrating': index % 2 == 0,
          'calibration_percent': index * 10 - 10, 'calibration_rpy': ([0., -.125, .25] if index > 3 else [0.] * index),
          'feedback_block': (0, 9, 10, 169, 170, 179, 180, 189, 190, 199, 200, 65535)[index],
          'free_space_percent': index * 10. + .5,
          'processes': [{'name': f'process-{i}', 'running': (i + index) % 3 == 0, 'should_be_running': i % 2 == 0} for i in range(5)],
          'health': [{'service': service, 'all_checks': (i + index) % 3 == 0} for i, service in enumerate(
            ('deviceState', 'roadCameraState', 'carState', 'driverCameraState', 'wideRoadCameraState', 'modelV2', 'liveCalibration')[:index % 8])],
          'angle_offset_valid': index >= 3, 'angle_offset': index + .25, 'steer_ratio_valid': index >= 6,
          'steer_ratio': 12. + index * .25, 'stiffness_factor_valid': index >= 9, 'stiffness_factor': .75,
          'cpu_temps': [] if index == 0 else [index * 10. + .5, 22.5], 'gpu_temps': [21.5], 'memory_temp': 20.5,
          'memory_usage_percent': index * 10 - 10, 'frame_drop_percent': index + .25,
          'accel': (index - 6) * .1, 'torque': (index - 6) * .125,
          'debug_text_1': 'Active 테스트' if index % 2 else 'Inactive 테스트', 'debug_text_2': '' if index % 3 else 'details',
        }
        base = {'snapshot': s, 'language': language, 'metric': index % 2 == 0,
                'soft_disable_time': (0, 1, 49, 50, 51, 300)[index % 6], 'personality': index % 4,
                'branch': 'dev', 'replay': index % 2 == 0, 'mici': mici,
                'params': {'NNFFModelName': '' if index % 2 else '검증 model', 'CanParserResult': None if index % 3 else 'parser\nresult',
                           'HyundaiCameraSCC': index % 2, 'HyundaiCameraSccHint': index % 2 == 0}}
        result.extend({**copy.deepcopy(base), 'callback': callback} for callback in callbacks.values())
  base = copy.deepcopy(result[-1])
  cases = {
    'min_enable_speed': 'below_engage_speed_alert', 'min_steer_speed': 'below_steer_speed_alert',
    'ego_speed': 'posenet_invalid_alert', 'model_velocity': 'posenet_invalid_alert',
    'free_space_percent': 'out_of_space_alert', 'angle_offset': 'paramsd_invalid_alert',
    'steer_ratio': 'paramsd_invalid_alert', 'stiffness_factor': 'paramsd_invalid_alert',
    'frame_drop_percent': 'modeld_lagging_alert', 'accel': 'joystick_alert', 'torque': 'joystick_alert',
    'memory_temp': 'overheat_alert', 'cpu_first': 'overheat_alert', 'cpu_last': 'overheat_alert', 'gpu_first': 'overheat_alert',
    'calibration_pitch': 'calibration_invalid_alert', 'calibration_yaw': 'calibration_invalid_alert',
  }
  for field, callback in cases.items():
    for language in languages:
      for value in ('nan', 'inf', 'neg_inf'):
        row = copy.deepcopy(base)
        row.update(callback={'name': callback}, language=language, nonfinite={'field': field, 'value': value})
        for valid in ('angle_offset_valid', 'steer_ratio_valid', 'stiffness_factor_valid'):
          row['snapshot'][valid] = valid != f'{field}_valid'
        result.append(row)
  for field, callback in cases.items():
    if field in ('model_velocity', 'cpu_first', 'cpu_last', 'gpu_first', 'calibration_pitch', 'calibration_yaw'):
      continue
    for value in (-1e300, -1e20, -2.5, -1.5, -0.5, -0.0, 0.0, 0.05, 0.15, 0.25, 0.35, 0.5, 1.5, 2.5, 1e20, 1e300):
      for adjacent in (math.nextafter(value, -math.inf), value, math.nextafter(value, math.inf)):
        row = copy.deepcopy(base)
        row.update(callback={'name': callback})
        row['snapshot'][field] = adjacent
        for valid in ('angle_offset_valid', 'steer_ratio_valid', 'stiffness_factor_valid'):
          row['snapshot'][valid] = valid != f'{field}_valid'
        result.append(row)
  for mici in (False, True):
    for language in languages:
      row = copy.deepcopy(base)
      row.update(callback={'name': 'torque_nn_load_alert'}, language=language, mici=mici, wire_check=True)
      row['params']['NNFFModelName'] = None
      result.append(row)
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  requests = rows()
  sources = {(row['mici'], row['language']): None for row in requests}
  for key in sources:
    sources[key] = setup_source(*key)
  expected = [original(row, sources[(row['mici'], row['language'])]) for row in requests]
  exceptions = [{'index': index, **row.pop('source_exception')} for index, row in enumerate(expected) if 'source_exception' in row]
  (args.output / 'source-exceptions.json').write_text(json.dumps(exceptions, indent=2))
  payload = ''.join(json.dumps(row) + '\n' for row in requests)
  (args.output / 'input.jsonl').write_text(payload)
  result = subprocess.run([str(args.binary.resolve()), str(ROOT / 'openpilot/selfdrive/ui/translations')],
                          input=payload, text=True, capture_output=True, timeout=120)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  (args.output / 'source.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
  assert result.returncode == 0, result.stderr
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(expected) == len(actual)
  failures = [{'index': i, 'request': requests[i], 'source': source, 'native': native}
              for i, (source, native) in enumerate(zip(expected, actual, strict=True)) if source != native]
  (args.output / 'differences.json').write_text(json.dumps(failures, indent=2, ensure_ascii=False))
  assert not failures, f'{len(failures)} callback differences; see differences.json'
  report = {'result': 'PASS', 'cases': len(requests), 'hardware_language_combinations': len(sources),
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'source_exceptions': len(exceptions),
            'source_hashes': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in [
              ROOT / 'openpilot/selfdrive/selfdrived/events.py', ROOT / 'openpilot/system/ui/lib/multilang.py',
              ROOT / 'openpilot/common/constants.py', ROOT / 'openpilot/common/realtime.py',
              ROOT / 'openpilot/selfdrive/locationd/calibrationd.py', ROOT / 'openpilot/system/micd.py',
              ROOT / 'openpilot/selfdrive/ui/feedback/feedbackd.py', ROOT / 'opendbc_repo/opendbc/car/hyundai/values.py',
              ROOT / 'openpilot/cereal/log.capnp', ROOT / 'opendbc_repo/opendbc/car/car.capnp',
              *sorted((ROOT / 'openpilot/selfdrive/ui/translations').glob('*.po')),
              ROOT / 'openpilot/selfdrive/ui/translations/languages.json',
            ]}}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()

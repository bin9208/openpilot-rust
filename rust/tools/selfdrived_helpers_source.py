"""Live SelfdriveD helper extraction retained independently of retired checker CLIs.

These functions are unchanged moves from check_alert_callbacks.py and
check_selfdrive_helpers.py (before retirement commit 3e715a34). They compile the
actual current project source/translation files; no captured callback or helper
results stand in for those classes. Fixed checker input generation and comparison
remain retired in native dev-tools.
"""
import ast
from enum import IntFlag, StrEnum, auto
import json
import logging
import math
from pathlib import Path
import re
from types import SimpleNamespace as NS

from generate_selfdrive_alerts import load_source
from openpilot.common.constants import CV
from openpilot.selfdrive.locationd.helpers import Pose

ROOT = Path(__file__).resolve().parents[2]


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


def literal_constant(path, name):
  tree = ast.parse((ROOT / path).read_text())
  node = next(node for node in tree.body if isinstance(node, ast.Assign)
              and any(isinstance(target, ast.Name) and target.id == name for target in node.targets))
  return ast.literal_eval(node.value)


def source_check():
  path = ROOT / 'openpilot/selfdrive/selfdrived/helpers.py'
  nodes = [node for node in ast.parse(path.read_text()).body if isinstance(node, (ast.Assign, ast.ClassDef))]
  namespace = {'StrEnum': StrEnum, 'auto': auto, 'math': math, 'car': NS(CarState=None), 'messaging': NS(SubMaster=None), 'Pose': Pose,
               'DT_CTRL': literal_constant('openpilot/common/realtime.py', 'DT_CTRL'),
               'ACCELERATION_DUE_TO_GRAVITY': literal_constant('opendbc_repo/opendbc/car/__init__.py', 'ACCELERATION_DUE_TO_GRAVITY'),
               'ISO_LATERAL_ACCEL': literal_constant('opendbc_repo/opendbc/car/lateral.py', 'ISO_LATERAL_ACCEL'),
               'ACCEL_MIN': literal_constant('opendbc_repo/opendbc/car/interfaces.py', 'ACCEL_MIN'),
               'ACCEL_MAX': literal_constant('opendbc_repo/opendbc/car/interfaces.py', 'ACCEL_MAX')}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  return namespace['ExcessiveActuationCheck']

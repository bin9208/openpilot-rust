#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Imported by: PYTHONPATH=. python rust/tools/check_car_specific.py --help
# ──────────────────
from __future__ import annotations

import ast
from collections import deque
from pathlib import Path
import sys
import subprocess
import hashlib
from types import SimpleNamespace

from openpilot.cereal import car, log
from generate_selfdrive_alerts import load_source

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/car/car_specific.py'
CP_FIELDS = ('brand', 'minSteerSpeed', 'minEnableSpeed', 'pcmCruise', 'openpilotLongitudinalControl', 'networkLocation')


class TraceParams:
  def __init__(self, params):
    self.params = params
    self.effects = []

  def get_bool(self, key):
    value = self.params.get_bool(key)
    self.effects.append({'operation': 'get_bool', 'key': key, 'value': value})
    return value

  def put_bool(self, key, value):
    result = self.params.put_bool(key, value)
    self.effects.append({'operation': 'put_bool', 'key': key, 'value': value})
    return result


def load(params):
  from opendbc.car import DT_CTRL, structs
  from opendbc.car.interfaces import MAX_CTRL_SPEED
  from opendbc.car.volkswagen.values import CarControllerParams
  from opendbc.car.hyundai.interface import ENABLE_BUTTONS
  from opendbc.car.hyundai.carstate import PREV_BUTTON_SAMPLES
  from openpilot.selfdrive.carrot.bluetooth.model import BLUETOOTH_CANCEL

  source = subprocess.check_output(['git', 'show', 'e8e152dbcfd9574a1791ed12026d1880e87689b3:openpilot/selfdrive/car/car_specific.py'], cwd=ROOT, text=True)
  tree = ast.parse(source)
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'CarSpecificEvents')
  namespace = {'car': car, 'structs': structs, 'deque': deque, 'DT_CTRL': DT_CTRL, 'MAX_CTRL_SPEED': MAX_CTRL_SPEED,
               'VWCarControllerParams': CarControllerParams, 'HYUNDAI_ENABLE_BUTTONS': ENABLE_BUTTONS,
               'HYUNDAI_PREV_BUTTON_SAMPLES': PREV_BUTTON_SAMPLES, 'BLUETOOTH_CANCEL': BLUETOOTH_CANCEL,
               'ButtonType': structs.CarState.ButtonEvent.Type, 'GearShifter': structs.CarState.GearShifter,
               'EventName': log.OnroadEvent.EventName, 'NetworkLocation': structs.CarParams.NetworkLocation,
               'Params': lambda: params, 'TESLA_STANDSTILL_STEER_FAULT_DEBOUNCE_FRAMES': 200, 'TESLA_STANDSTILL_STEER_FAULT_SPEED': 0.5}
  events = load_source('tici')
  namespace.update(Events=events['Events'], ET=events['ET'])
  exec(compile(ast.Module(body=[cls], type_ignores=[]), str(SOURCE), 'exec'), namespace)
  add_lines = {node.lineno for node in ast.walk(cls) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
               and node.func.attr == 'add' and isinstance(node.func.value, ast.Name) and node.func.value.id == 'events'}
  conditions = [(node.lineno, node.test.end_lineno, node.body[0].lineno) for node in ast.walk(cls) if isinstance(node, ast.If)]
  return SimpleNamespace(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', source_sha256=hashlib.sha256(source.encode()).hexdigest(), cls=namespace['CarSpecificEvents'], add_lines=add_lines, conditions=conditions,
                         constants={'DT_CTRL': DT_CTRL, 'MAX_CTRL_SPEED': MAX_CTRL_SPEED,
                                    'VW_DEFAULT_MIN_STEER_SPEED': CarControllerParams.DEFAULT_MIN_STEER_SPEED,
                                    'HYUNDAI_ENABLE_BUTTONS': list(ENABLE_BUTTONS), 'HYUNDAI_PREV_BUTTON_SAMPLES': PREV_BUTTON_SAMPLES,
                                    'BLUETOOTH_CANCEL': BLUETOOTH_CANCEL})


def snapshot(machine):
  if machine is None:
    return None
  data = {key: value for key, value in vars(machine).items() if key not in ('CP', 'params')}
  data['CP'] = {field: getattr(machine.CP, field) for field in CP_FIELDS}
  data['CP']['networkLocation'] = machine.CP.networkLocation.raw
  data['cruise_buttons'] = list(machine.cruise_buttons)
  return data


def wire(module, data):
  message = module.new_message(**data)
  raw = message.to_bytes()
  with module.from_bytes(raw) as reader:
    return raw, reader.as_builder()


class SourceTrace:
  def __init__(self):
    self.lines = set()
    self.arcs = set()
    self.previous = {}

  def trace(self, frame, event, _arg):
    if frame.f_code.co_filename == str(SOURCE):
      if event == 'line':
        self.lines.add(frame.f_lineno)
        previous = self.previous.get(id(frame))
        if previous is not None:
          self.arcs.add((previous, frame.f_lineno))
        self.previous[id(frame)] = frame.f_lineno
      elif event == 'return':
        previous = self.previous.pop(id(frame), None)
        if previous is not None:
          self.arcs.add((previous, -frame.f_code.co_firstlineno))
    return self.trace

  def run(self, function):
    sys.settrace(self.trace)
    try:
      return function()
    finally:
      sys.settrace(None)

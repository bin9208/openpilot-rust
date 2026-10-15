#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp", "pyzmq"]
# ///
# ─── How to run ───
# Imported by: PYTHONPATH=. python rust/tools/check_selfdrived_controller.py --help
# ──────────────────
"""Complete pinned final SelfdriveD AST with controlled OS/IPC adapters."""
from __future__ import annotations

import ast
import copy
from collections import defaultdict
from dataclasses import dataclass
import json
import os
import sys
from types import ModuleType
import subprocess
from pathlib import Path
from types import SimpleNamespace as NS
from contextlib import ExitStack

from openpilot.cereal import car, log, messaging
from openpilot.selfdrive.locationd.helpers import Pose, PoseCalibrator
from check_selfdrive_helpers import get_camera_packets
from openpilot.selfdrive.controls.lib.cutin_alert import CutinAlertCandidate, CutinAlertTracker, promoted_cutin_candidates
from check_alert_callbacks import setup_source
from check_selfdrive_helpers import source_check
from car_specific_oracle import load as car_source

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/selfdrived/selfdrived.py'

DM2_PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'

def final_controller_nodes(_nodes):
  # The complete final class is executed, including startup, impact, DM and advisory.
  final_text = subprocess.check_output(['git','show',f'{DM2_PIN}:openpilot/selfdrive/selfdrived/selfdrived.py'],cwd=ROOT,text=True)
  cls = next(n for n in ast.parse(final_text).body if isinstance(n,ast.ClassDef) and n.name=='SelfdriveD')
  init=next(n for n in cls.body if isinstance(n,ast.FunctionDef) and n.name=='__init__')
  # Retained native diagnostic field has no runtime authority; final source removed it.
  init.body.append(ast.Assign(targets=[ast.Attribute(value=ast.Name(id='self',ctx=ast.Load()),attr='disable_dm',ctx=ast.Store())],value=ast.Constant(0)))
  # Preserve the independent Rust-base Jetlink safety contract alongside final upstream.
  baseline=ast.parse(SOURCE.read_text())
  old=next(n for n in baseline.body if isinstance(n,ast.ClassDef) and n.name=='SelfdriveD')
  old_events=next(n for n in old.body if isinstance(n,ast.FunctionDef) and n.name=='update_events')
  final_events=next(n for n in cls.body if isinstance(n,ast.FunctionDef) and n.name=='update_events')
  required=next(n for n in old_events.body if isinstance(n,ast.ImportFrom) and n.module.endswith('jetlink.transition'))
  filter_node=copy.deepcopy(next(n for n in old_events.body if isinstance(n,ast.Assign) and ast.unparse(n.targets[0])=='not_running'))
  filter_node.value.args[0]=ast.Name(id='not_running',ctx=ast.Load())
  lost_start=next(i for i,n in enumerate(old_events.body) if isinstance(n,ast.Assign) and ast.unparse(n.targets[0])=='jetlink_status')
  body=[]
  for node in final_events.body:
    if isinstance(node,ast.Assign) and ast.unparse(node.targets[0])=='has_disable_events':
      body.extend(copy.deepcopy(old_events.body[lost_start:lost_start+2]))
    body.append(node)
    if isinstance(node,ast.Assign) and ast.unparse(node.targets[0])=='not_running':
      body.extend([copy.deepcopy(required),filter_node])
  final_events.body=body
  return ast.fix_missing_locations(ast.Module(body=[cls],type_ignores=[]))


class TraceParams:
  """Record calls while retaining the physical Cython Params conversion and writes."""
  def __init__(self, params, effects):
    self.params, self.effects = params, effects

  def get(self, key, return_default=False):
    self.effects.append({'operation': 'get', 'key': key, 'return_default': return_default})
    return self.params.get(key, return_default=return_default)

  def get_bool(self, key):
    self.effects.append({'operation': 'get_bool', 'key': key})
    return self.params.get_bool(key)

  def get_int(self, key):
    self.effects.append({'operation': 'get_int', 'key': key})
    return self.params.get_int(key)

  def put_bool(self, key, value):
    self.effects.append({'operation': 'put_bool', 'key': key, 'value': value})
    return self.params.put_bool(key, value)

  def remove(self, key):
    self.effects.append({'operation': 'remove', 'key': key})
    return self.params.remove(key)

  def put(self, key, value):
    return self.params.put(key, value)


class Fixture:
  """Own a source instance; mutation supplies incoming IO and records its effects."""
  def __init__(self, params):
    self.effects, self.messages = [], []
    self.params = TraceParams(params, self.effects)
    self.now, self.current, self.pending, self.streams = 0.0, None, [], []
    self.machine = None
    from selfdrived_final_dependencies import Memory
    self.memory = Memory()

  def sleep(self, _duration):
    self.params_stop = True

  def stop_event(self):
    return NS(is_set=lambda: self.params_stop)

  def monotonic(self):
    self.effects.append({'operation': 'monotonic', 'value': self.now})
    return self.now

  def streams_available(self, *_args, **_kwargs):
    self.effects.append({'operation': 'available_streams'})
    return self.streams.copy()

  def event(self, name, **fields):
    self.effects.append({'operation': 'event', 'name': name, 'fields': json.dumps(fields, default=repr)})

  def send(self, topic, message):
    self.messages.append({'topic': topic, 'bytes': list(message.to_bytes())})

  def new_message(self, *args):
    message = messaging.new_message(*args)
    message.logMonoTime = int(self.now * 1e9)
    return message

  def submaster(self, *args, **kwargs):
    from check_message_state import source
    scope, environment=source()
    environment['simulation']=os.environ.get('SIMULATION','0')
    text=subprocess.check_output(['git','show',f'{DM2_PIN}:openpilot/cereal/messaging/__init__.py'],cwd=ROOT,text=True)
    nodes=[n for n in ast.parse(text).body if isinstance(n,(ast.FunctionDef,ast.ClassDef)) and n.name in ('new_message','FrequencyTracker','SubMaster')]
    scope['Poller']=messaging.Poller
    exec(compile(ast.Module(body=nodes,type_ignores=[]),'final-source:messaging','exec'),scope)
    result=scope['SubMaster'](*args,**kwargs)
    result.update=lambda _timeout:result.update_msgs(self.now,self.pending)
    return result

  def offroad(self, key, show, extra_text=None):
    if show:
      self.effects.append({'operation': 'offroad', 'key': key, 'extra': extra_text})
    self.original_offroad(key, show, extra_text)

  def initialize(self, cp, mode, language):
    self.machine = None
    # Filesystem read is controlled; source fault_active remains covered by its native adapter tests.
    link=ModuleType('openpilot.selfdrive.modeld.jetlink.link')
    link.fault_active=lambda:False
    sys.modules[link.__name__]=link
    events = setup_source(mode['device_type'] == 'mici', language or 'en')
    events['Params'] = lambda: self.params
    timing = ROOT / 'openpilot/common/realtime.py'
    rate = next(n for n in ast.parse(timing.read_text()).body if isinstance(n, ast.ClassDef) and n.name == 'Ratekeeper')
    utils = ROOT / 'openpilot/common/utils.py'
    average = next(n for n in ast.parse(utils.read_text()).body if isinstance(n, ast.ClassDef) and n.name == 'MovingAverage')
    rates = {'time': NS(monotonic=self.monotonic, sleep=self.sleep), 'getproctitle': lambda: 'fixture'}
    exec(compile(ast.Module(body=[average, rate], type_ignores=[]), str(timing), 'exec'), rates)
    alert_path = ROOT / 'openpilot/selfdrive/selfdrived/alertmanager.py'
    nodes = [n for n in ast.parse(alert_path.read_text()).body if isinstance(n, (ast.ClassDef, ast.FunctionDef))]
    alert_namespace = {'copy': copy, 'json': json, 'defaultdict': defaultdict, 'dataclass': dataclass,
                       'Alert': events['Alert'], 'EmptyAlert': events['EmptyAlert'], 'Params': lambda: self.params,
                       'OFFROAD_ALERTS': json.loads((alert_path.parent / 'alerts_offroad.json').read_text())}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(alert_path), 'exec'), alert_namespace)
    self.original_offroad = alert_namespace['set_offroad_alert']
    diagnostics_path = ROOT / 'openpilot/common/runtime_diagnostics.py'
    diagnostic = next(n for n in ast.parse(diagnostics_path.read_text()).body if isinstance(n, ast.FunctionDef) and n.name == 'communication_snapshot')
    diagnostics = {'time': NS(monotonic=self.monotonic, sleep=self.sleep)}
    exec(compile(ast.Module(body=[diagnostic], type_ignores=[]), str(diagnostics_path), 'exec'), diagnostics)
    car_events = car_source(self.params).cls
    car_events.__init__.__globals__['Events'] = events['Events']
    tree = ast.parse(SOURCE.read_text())
    nodes = [n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == 'SelfdriveD']
    namespace = {'car': car, 'log': log, 'time': NS(monotonic=self.monotonic, sleep=self.sleep), 'Params': lambda *args: self.memory if args else self.params,
                 'get_build_metadata': lambda: None, 'CarSpecificEvents': car_events, 'PoseCalibrator': PoseCalibrator,
                 'Pose': Pose, 'ExcessiveActuationCheck': source_check(), 'get_camera_packets': get_camera_packets,
                 'get_gps_location_service': lambda p: 'gpsLocationExternal' if p.get_bool('UbloxAvailable') else 'gpsLocation',
                 'messaging': NS(PubMaster=lambda _names: NS(send=self.send), SubMaster=self.submaster, sub_sock=lambda *_args, **_kw: None,
                                 new_message=self.new_message, recv_one=lambda _sock: NS(carState=self.current,logMonoTime=int(self.now*1e9)) if self.current is not None else None),
                 'Events': events['Events'], 'ET': events['ET'], 'EmptyAlert': events['EmptyAlert'], 'AlertManager': alert_namespace['AlertManager'],
                 'StateMachine': self.state_machine_class(), 'Ratekeeper': rates['Ratekeeper'], 'set_offroad_alert': self.offroad,
                 'CutinAlertCandidate': CutinAlertCandidate, 'CutinAlertTracker': CutinAlertTracker, 'promoted_cutin_candidates': promoted_cutin_candidates,
                 'VisionIpcClient': NS(available_streams=self.streams_available),
                 'VisionStreamType': NS(VISION_STREAM_ROAD=0, VISION_STREAM_WIDE_ROAD=2),
                 'HARDWARE': NS(get_device_type=lambda: mode['device_type']), 'cloudlog': NS(event=self.event),
                 'os': NS(path=NS(exists=lambda _path: mode['nvme_present'])), 'communication_snapshot': diagnostics['communication_snapshot'],
                 'DT_CTRL': events['DT_CTRL'], 'REPLAY': mode['replay'], 'SIMULATION': mode['simulation'], 'TESTING_CLOSET': mode['testing_closet']}
    for node in tree.body:
      if isinstance(node, ast.Assign) and not any(isinstance(t, ast.Name) and t.id in ('REPLAY', 'SIMULATION', 'TESTING_CLOSET') for t in node.targets):
        exec(compile(ast.Module(body=[node], type_ignores=[]), str(SOURCE), 'exec'), namespace)
    from selfdrived_final_dependencies import load
    namespace.update(load())
    namespace['cloudlog'].warning = lambda *_args,**_kwargs: None
    namespace['cloudlog'].exception = lambda *_args,**_kwargs: None
    exec(compile(final_controller_nodes(nodes), str(SOURCE) + ':final-controller', 'exec'), namespace)
    self.machine = namespace['SelfdriveD'](cp)
    self.mode = mode

  @staticmethod
  def state_machine_class():
    path = ROOT / 'openpilot/selfdrive/selfdrived/state.py'
    nodes = [n for n in ast.parse(path.read_text()).body if not isinstance(n, (ast.Import, ast.ImportFrom))]
    namespace = {'log': log, 'Events': None, 'ET': setup_source(False, 'en')['ET'], 'DT_CTRL': 0.01}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
    return namespace['StateMachine']

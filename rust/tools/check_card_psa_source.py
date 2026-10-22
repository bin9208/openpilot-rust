#!/usr/bin/env python3
"""Hosted PSA primary/helper and immutable baseline failure controls (#501)."""
from __future__ import annotations

import argparse
import ast
import contextlib
import hashlib
import io
import itertools
import json
from pathlib import Path
import subprocess
import sys
import traceback
import types
import urllib.request
from can_source import ROOT, load
from card_qa.mazda.source import Settings

BASELINE='b3705789764410d182e7bc9ac384634fa633e6cd'
PRIMARY='c3955753ae7258412814e2cdca3ee8af979e4679'
PRIMARY_BLOB='34d557e5b535f3333ed1101a747c8d68f08a3ab1'


def method(source):
  owner=next(node for node in ast.parse(source).body if isinstance(node,ast.ClassDef) and node.name=='CarStateBase')
  return next(node for node in owner.body if isinstance(node,ast.FunctionDef) and node.name=='parse_wheel_speeds')


def controls(output):
  from opendbc.car import Bus, interfaces, structs
  from opendbc.car.psa.interface import CarInterface
  url=f'https://raw.githubusercontent.com/commaai/opendbc/{PRIMARY}/opendbc/car/interfaces.py'
  with urllib.request.urlopen(url,timeout=60) as response: primary=response.read()
  assert hashlib.sha1(f'blob {len(primary)}\0'.encode()+primary).hexdigest()==PRIMARY_BLOB
  (output/'primary-interfaces.py').write_bytes(primary)
  current=(ROOT/'opendbc_repo/opendbc/car/interfaces.py').read_bytes()
  assert ast.dump(method(primary),include_attributes=False)==ast.dump(method(current),include_attributes=False)
  paths=['opendbc_repo/opendbc/car/interfaces.py','opendbc_repo/opendbc/car/psa/carstate.py']+[
    f'opendbc_repo/opendbc/car/torque_data/{name}.toml' for name in ('params','override','substitute')]
  source={path:subprocess.check_output(['git','show',f'{BASELINE}:{path}'],cwd=ROOT) for path in paths}
  directory=output/'baseline-source'; directory.mkdir()
  for path,data in source.items(): (directory/Path(path).name).write_bytes(data)
  old=types.ModuleType('_psa_501_baseline_interfaces'); sys.modules[old.__name__]=old
  try:
    exec(compile(source[paths[0]],f'{BASELINE}:{paths[0]}','exec'),old.__dict__)
    old.TORQUE_PARAMS_PATH=directory/'params.toml'; old.TORQUE_OVERRIDE_PATH=directory/'override.toml'
    old.TORQUE_SUBSTITUTE_PATH=directory/'substitute.toml'
    settings=Settings({}); old.Params=lambda:settings
    try: old.CarInterfaceBase.get_params.__func__(CarInterface,'PSA_PEUGEOT_208',{bus:{} for bus in range(8)},[],False,True,False)
    except KeyError as error:
      assert error.args==('PSA_PEUGEOT_208',)
      (output/'baseline-params.traceback.log').write_text(traceback.format_exc())
    else: raise AssertionError('baseline unexpectedly admitted PSA torque')
    assert not settings.writes
    # Execute the original class body with its actual original base class, not the repaired base.
    scope={}; exec(compile(source[paths[1]],f'{BASELINE}:{paths[1]}','exec'),scope)
    scope['CarStateBase']=old.CarStateBase
    node=next(node for node in ast.parse(source[paths[1]]).body if isinstance(node,ast.ClassDef) and node.name=='CarState')
    exec(compile(ast.Module(body=[node],type_ignores=[]),f'{BASELINE}:{paths[1]}','exec'),scope)
    state=scope['CarState'](structs.CarParams.new_message(carFingerprint='PSA_PEUGEOT_208'))
    try: state.update({Bus.main:object(),Bus.adas:object(),Bus.cam:object()})
    except AttributeError as error:
      assert str(error)=="'CarState' object has no attribute 'parse_wheel_speeds'"
      (output/'baseline-state.traceback.log').write_text(traceback.format_exc())
    else: raise AssertionError('baseline unexpectedly had parse_wheel_speeds')
  finally: sys.modules.pop(old.__name__)
  from check_tesla_model_x_497 import finite
  rows=[]
  for nnff,alpha in itertools.product((False,True),repeat=2):
    settings=Settings({'NNFF':str(int(nnff))}); interfaces.Params=lambda:settings
    cp=CarInterface.get_params('PSA_PEUGEOT_208',{bus:{} for bus in range(8)},[],alpha,True,False)
    assert cp.dashcamOnly and cp.radarUnavailable and not cp.openpilotLongitudinalControl
    assert cp.steerControlType==structs.CarParams.SteerControlType.angle and cp.lateralTuning.which()=='pid'
    assert cp.maxLateralAccel==2.0; finite(cp.to_dict())
    vehicle=CarInterface(cp)
    for _ in range(33): finite(vehicle.update([]).to_dict())
    assert list(vehicle.can_parsers)==[Bus.main,Bus.adas,Bus.cam]
    assert [p.bus for p in vehicle.can_parsers.values()]==[0,1,2]
    assert not any(write[0]=='NNFFModelName' for write in settings.writes)
    rows.append(dict(nnff=nnff,alpha_long=alpha,updates=33,writes=settings.writes))
  import opendbc.can.dbc as dbc
  original=dbc.DBC_PATH; empty=output/'empty-dbc'; empty.mkdir(); settings=Settings({}); interfaces.Params=lambda:settings
  try:
    dbc.DBC_PATH=str(empty); dbc.DBC.cache_clear()
    try: CarInterface(cp)
    except FileNotFoundError:
      (output/'missing-dbc.traceback.log').write_text(traceback.format_exc())
    else: raise AssertionError('actual source unexpectedly constructed without the PSA DBC')
    assert not settings.writes
  finally: dbc.DBC_PATH=original; dbc.DBC.cache_clear()
  return dict(primary_commit=PRIMARY,primary_blob=PRIMARY_BLOB,method_body_matches_primary=True,
    baseline=BASELINE,baseline_sha256={path:hashlib.sha256(data).hexdigest() for path,data in source.items()},
    source_cases=rows,source_controls='pass',native_runtime_gate_complete=False,physical_can_validated=False)


def main():
  parser=argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--evidence',type=Path,required=True)
  parser.add_argument('--binary',type=Path)
  parser.add_argument('--numerics',type=Path)
  args=parser.parse_args()
  if args.binary is not None and args.numerics is None: parser.error('--binary requires --numerics for full PSA/Common comparison')
  args.evidence.mkdir(parents=True,exist_ok=False); load(); captured=io.StringIO(); report={}
  try:
    with contextlib.redirect_stdout(captured),contextlib.redirect_stderr(captured): report=controls(args.evidence)
    if args.binary is not None:
      command=[sys.executable,str(ROOT/'rust/tools/check_card_psa.py'),'--binary',str(args.binary.resolve()),
        '--numerics',str(args.numerics.resolve()),'--evidence',str(args.evidence/'runtime')]
      (args.evidence/'runtime-command.json').write_text(json.dumps(command)+'\n')
      child=subprocess.run(command,text=True,capture_output=True,check=False)
      (args.evidence/'runtime-process.log').write_text(child.stdout+child.stderr+f'\nEXIT {child.returncode}\n')
      child.check_returncode(); report['native_runtime_gate_complete']=True
  finally:
    (args.evidence/'source.log').write_text(captured.getvalue())
    (args.evidence/'partial.json').write_text(json.dumps(report,indent=2)+'\n')
  (args.evidence/'result.json').write_text(json.dumps(report,indent=2)+'\n'); print(json.dumps(report))


if __name__=='__main__':main()

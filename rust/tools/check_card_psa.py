#!/usr/bin/env python3
"""Hosted comparison of the actual PSA dashcam interface/state/controller (#501)."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT, load
from card_qa.psa.scenarios import cases
from card_qa.psa.source import trace
from check_card_mazda import compare, decoded as decode_fields


def decoded(value):
  present = "error" in value
  error = value.pop("error", None)
  value = decode_fields(value)
  if present: value["error"] = error
  return value


def main():
  parser=argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary',type=Path,required=True)
  parser.add_argument('--numerics',type=Path,required=True)
  parser.add_argument('--evidence',type=Path,required=True)
  parser.add_argument('--op',choices=['params','runtime','before_update','numeric_error'])
  args=parser.parse_args(); args.evidence.mkdir(parents=True,exist_ok=True)
  (args.evidence/'result.json').unlink(missing_ok=True)
  load(); captured=io.StringIO()
  try:
    with contextlib.redirect_stdout(captured),contextlib.redirect_stderr(captured):
      request=[c for c in cases() if args.op is None or c['op']==args.op]
      expected=[trace(case) for case in request]
  finally: (args.evidence/'source.log').write_text(captured.getvalue())
  assert request
  (args.evidence/'input.json').write_text(json.dumps(request)+'\n')
  (args.evidence/'source.json').write_text(json.dumps(expected)+'\n')
  target=args.evidence.resolve()/'native.json'; target.unlink(missing_ok=True)
  command=[str(args.binary.resolve()),str(target),str(ROOT/'opendbc_repo/opendbc/dbc'),
           str(ROOT/'opendbc_repo/opendbc/car/torque_data'),str(args.numerics.resolve())]
  (args.evidence/'command.json').write_text(json.dumps(command)+'\n')
  child=subprocess.run(command,input=json.dumps(request),text=True,capture_output=True,check=False)
  (args.evidence/'process.log').write_text(child.stdout+child.stderr+f'\nEXIT {child.returncode}\n')
  child.check_returncode(); actual=json.loads(target.read_text())
  prints=iter(child.stdout.splitlines())
  for value in actual:
    if 'steps' in value: value['prints']=[next(prints),*value['prints']]
  assert list(prints)==[]
  raw_failures=[]
  for case,left,right in zip(request,expected,actual,strict=True):
    if case['op']=='params':continue
    error=left['error']; raw_failures.append({'name':case['name'],'source':error,'native':right['error']})
    if error is None: assert right['error'] is None
    elif error['kind']=='AttributeError':
      assert error['message']=="'CarState' object has no attribute 'eps_active'"
      assert right['error']=={'kind':'Stock','attribute':'eps_active'}
    else:
      assert error['kind']=='ValueError' and 'NaN' in error['message']
      assert right['error']=={'kind':'Numeric'}
    if case['op']=='runtime' or case['name']=='before-update-False':assert error is None
    if case['name']=='before-update-True' or case['op']=='numeric_error':assert error is not None
    left['error']=right['error']
  left=[decoded(value) for value in expected]; right=[decoded(value) for value in actual]
  for case,one in zip(request,left,strict=True):
    cp=one['params']
    assert cp['dashcamOnly'] and cp['radarUnavailable'] and cp['steerControlType']=='angle'
    assert not cp['openpilotLongitudinalControl'] and not cp['alphaLongitudinalAvailable']
    assert cp['maxLateralAccel']==2.0 and cp['safetyConfigs'][0]['safetyModel']=='psa'
    if case['op']=='runtime':
      assert len(one['steps'])==360 and one['steps'][-1]['controller']['frame']==360
      for tick,step in enumerate(one['steps']):
        assert len(step['can'])==int(tick%5==0)
        assert all(frame['bus']==0 and frame['address']==1010 for frame in step['can'])
      assert one['steps'][0]['state']['vEgoRaw']==0.
      assert any(step['state']['vEgoRaw']>0 for step in one['steps'])
      assert {step['controller']['status'] for step in one['steps']} >= {2,3,4}
  (args.evidence/'source-fields.json').write_text(json.dumps(left)+'\n')
  (args.evidence/'native-fields.json').write_text(json.dumps(right)+'\n')
  (args.evidence/'failure-controls.json').write_text(json.dumps(raw_failures,indent=2)+'\n')
  try: compare(left,right)
  except AssertionError as error:
    (args.evidence/'failure.txt').write_text(str(error)+'\n');raise
  files=list((ROOT/'opendbc_repo/opendbc/car/psa').glob('*.py'))+[
    ROOT/'opendbc_repo/opendbc/car/interfaces.py',ROOT/'opendbc_repo/opendbc/car/lateral.py',
    ROOT/'opendbc_repo/opendbc/car/torque_data/override.toml',ROOT/'opendbc_repo/opendbc/dbc/psa_aee2010_r3.dbc']
  report=dict(status='pass',issue=501,cases=len(request),updates=sum(len(c['steps']) for c in request if c['op']=='runtime'),
    runtime_python=False,physical_can_validated=False,binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    source_sha256={str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in files},
    observable='full CP/CS/actuators, actual first/continuous updates, 3 parser histories, controller state/counters/CAN, logs, Common Params and lifecycle')
  (args.evidence/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))


if __name__=='__main__':main()

#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: verified oracle Python rust/tools/check_card_tesla.py --binary PATH --numerics DIR --evidence DIR
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT,load
from card_vehicle_source import normalize
from card_qa.tesla.source import trace
from card_qa.tesla.scenarios import cases


def byte_fields(value):
    if isinstance(value,bytes):return list(value)
    if isinstance(value,dict):return {key:byte_fields(item) for key,item in value.items()}
    if isinstance(value,list):return [byte_fields(item) for item in value]
    return value


def decoded(value):
    load()
    from openpilot.cereal import car
    if 'error' in value:return value
    with car.CarParams.from_bytes(bytes(value['params'])) as cp:value['params']=normalize(cp.to_dict())
    for step in value.get('steps',[]):
        with car.CarState.from_bytes(bytes(step['state'])) as cs:step['state']=normalize(cs.to_dict())
        with car.CarControl.Actuators.from_bytes(bytes(step['actuators'])) as a:step['actuators']=normalize(a.to_dict())
    return byte_fields(normalize(value))


def compare(left,right,path='root'):
    if isinstance(left,dict) and isinstance(right,dict):
        assert left.keys()==right.keys(),(path,left.keys(),right.keys())
        for key in left:compare(left[key],right[key],f'{path}.{key}')
    elif isinstance(left,list) and isinstance(right,list):
        assert len(left)==len(right),(path,len(left),len(right))
        for i,(a,b) in enumerate(zip(left,right,strict=True)):compare(a,b,f'{path}[{i}]')
    else:
        assert left==right,(path,left,right)


def main()->None:
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--numerics',type=Path,required=True);parser.add_argument('--evidence',type=Path,required=True);parser.add_argument('--op',choices=['params','runtime'])
    args=parser.parse_args();args.evidence.mkdir(parents=True,exist_ok=True);(args.evidence/'result.json').unlink(missing_ok=True);(args.evidence/'failure.txt').unlink(missing_ok=True)
    request=[c for c in cases() if args.op is None or c['op']==args.op];assert request
    source_log=io.StringIO()
    with contextlib.redirect_stdout(source_log),contextlib.redirect_stderr(source_log):expected=[trace(case) for case in request]
    (args.evidence/'input.json').write_text(json.dumps(request)+'\n');(args.evidence/'source.json').write_text(json.dumps(expected)+'\n');(args.evidence/'source.log').write_text(source_log.getvalue() or 'no source diagnostics\n')
    output=args.evidence/'native.json';output.unlink(missing_ok=True)
    child=subprocess.run([args.binary.resolve(),output.resolve(),ROOT/'opendbc_repo/opendbc/dbc',ROOT/'opendbc_repo/opendbc/car/torque_data',args.numerics.resolve()],input=json.dumps(request),text=True,capture_output=True,check=False)
    (args.evidence/'process.log').write_text(child.stdout+child.stderr+f'\nEXIT {child.returncode}\n');child.check_returncode()
    actual=json.loads(output.read_text());common_prints=iter(child.stdout.splitlines())
    for case in actual:
        if 'steps' in case:case['prints']=[next(common_prints),*case['prints']]
    assert list(common_prints)==[], 'unexpected native constructor prints'
    left=[decoded(v) for v in expected];right=[decoded(v) for v in actual]
    (args.evidence/'source-fields.json').write_text(json.dumps(left)+'\n');(args.evidence/'native-fields.json').write_text(json.dumps(right)+'\n')
    try:compare(left,right)
    except AssertionError as error:(args.evidence/'failure.txt').write_text(str(error)+'\n');raise
    files=list((ROOT/'opendbc_repo/opendbc/car/tesla').glob('*.py'))+[ROOT/'opendbc_repo/opendbc/car/interfaces.py',ROOT/'opendbc_repo/opendbc/car/vehicle_model.py',ROOT/'opendbc_repo/opendbc/car/__init__.py',ROOT/'opendbc_repo/opendbc/dbc/tesla_model3_party.dbc',ROOT/'opendbc_repo/opendbc/dbc/tesla_model3_vehicle.dbc']
    report=dict(status='pass',cases=len(request),frames=sum(len(c['steps']) for c in request),observable='full CarParams/CarState/Actuator wire fields; exact CAN frames and cadence; temporal Tesla/cooperative/speed-wheel state; severity/order of logs; lifecycle/constructor Params effects',runtime_python=False,binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),source_sha256={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files})
    (args.evidence/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))


if __name__=='__main__':main()

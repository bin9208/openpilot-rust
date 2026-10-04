import argparse
import ast
from contextlib import redirect_stderr,redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace
from can_source import ROOT,load
from check_card_vehicle import compare
from card_vehicle_source import normalize
from check_message_state import source as messaging_source


def settings():
  class Params:
    def get_bool(self,key):return False
    def put_int(self,key,value):assert key=='LongitudinalPersonalityMax' and value==3
  return Params()


def cases() -> list[dict]:
  load()
  from opendbc.can import CANPacker
  from openpilot.cereal import car,log
  packer=CANPacker('comma_body');rng=random.Random(177);steps=[]
  for index in range(3000):
    now=1_000_000_000+index*10_000_000
    control=car.CarControl.new_message(enabled=index%250<200,actuators=dict(accel=rng.uniform(-5,5),torque=rng.uniform(-1,1),steeringAngleDeg=.5))
    frames=[]
    for name,values in [('MOTORS_DATA',dict(SPEED_L=rng.randint(-100,100),SPEED_R=rng.randint(-100,100))),('VAR_VALUES',dict(FAULT=index%7==0)),('BODY_DATA',dict(BATT_PERCENTAGE=index%128,CHARGER_CONNECTED=index%2))]:
      address,data,bus=packer.make_can_msg(name,0,values)
      if index%71==70 and name=='MOTORS_DATA':data=bytes([data[0]^1])+data[1:]
      frames.append(dict(address=address,data=list(data),bus=bus))
    packets=[] if index%31==30 else [dict(mono_time=now,frames=frames)]
    steps.append(dict(now=now,packets=packets,control=list(control.to_bytes()),gps=[]))
  result=[dict(brand='body',now=1_000_000_000,steps=steps)]
  steps=[]
  for index in range(400):
    now=1_000_000_000+index*10_000_000;messages=[]
    for name,speed in [('gpsLocation',index/10),('gpsLocationExternal',index/20)]:
      if index%3==0 or (name=='gpsLocationExternal' and index%13==0):
        event=log.Event.new_message(valid=False,logMonoTime=now);event.init(name);getattr(event,name).speed=speed;messages.append(list(event.to_bytes()))
    control=car.CarControl.new_message(enabled=index%2==0,actuators=dict(accel=.25,torque=.5))
    steps.append(dict(now=now,packets=[],control=list(control.to_bytes()),gps=messages))
  result.append(dict(brand='mock',now=1_000_000_000,steps=steps));return result


def trace(case:dict) -> dict:
  load()
  from opendbc.car import interfaces
  interfaces.Params=lambda:settings()
  module=__import__(f'opendbc.car.{case["brand"]}.interface',fromlist=['CarInterface'])
  candidate='COMMA_BODY' if case['brand']=='body' else 'MOCK'
  cp=module.CarInterface.get_params(candidate,{},[],False,True,False)
  import opendbc.can.parser as parser
  current=case['now'];parser.time=type('Clock',(),{'monotonic_ns':staticmethod(lambda:current)})
  vehicle=module.CarInterface(cp);results=[]
  gps_scope,_=messaging_source();gps=gps_scope['SubMaster'](['gpsLocation','gpsLocationExternal'])
  from openpilot.cereal import car,log
  input=case['steps'][0]
  gps.update=lambda timeout:gps.update_msgs(current/1e9,list(log.Event.read_multiple_bytes(b''.join(bytes(message) for message in input['gps']))))
  path=ROOT/'openpilot/selfdrive/car/car_specific.py'
  node=next(node for node in ast.parse(path.read_text()).body if isinstance(node,ast.ClassDef) and node.name=='MockCarState')
  scope=dict(car=car,messaging=SimpleNamespace(SubMaster=lambda topics:gps))
  exec(compile(ast.Module(body=[node],type_ignores=[]),str(path),'exec'),scope)
  mock_state=scope['MockCarState']()
  for input in case['steps']:
    current=input['now']
    packets=[(packet['mono_time'],[(frame['address'],bytes(frame['data']),frame['bus']) for frame in packet['frames']]) for packet in input['packets']]
    state=vehicle.update(packets)
    if case['brand']=='mock':
      state=mock_state.update(state)
    with car.CarControl.from_bytes(bytes(input['control'])) as control:actuators,can=vehicle.apply(control,current)
    results.append(dict(state=list(state.to_bytes()),actuators=list(actuators.to_bytes()),can=[dict(address=address,data=list(data),bus=bus) for address,data,bus in can]))
  return dict(params=list(cp.to_bytes()),steps=results)


def decoded(value:dict) -> dict:
  from openpilot.cereal import car
  with car.CarParams.from_bytes(bytes(value['params'])) as cp:value['params']=normalize(cp.to_dict())
  for step in value['steps']:
    with car.CarState.from_bytes(bytes(step['state'])) as state:step['state']=normalize(state.to_dict())
    with car.CarControl.Actuators.from_bytes(bytes(step['actuators'])) as actuators:step['actuators']=normalize(actuators.to_dict())
  return value


def main() -> None:
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--evidence',type=Path,required=True)
  args=parser.parse_args();args.evidence.mkdir(parents=True,exist_ok=True);request=cases();source_log=io.StringIO()
  with redirect_stdout(source_log),redirect_stderr(source_log):expected=[trace(case) for case in request]
  (args.evidence/'input.json').write_text(json.dumps(request)+'\n');(args.evidence/'source.json').write_text(json.dumps(expected)+'\n');(args.evidence/'source.log').write_text(source_log.getvalue() or 'no source diagnostics\n')
  target=args.evidence/'native.json';dbc=ROOT/'opendbc_repo/opendbc/dbc'
  child=subprocess.run([args.binary.resolve(),target.resolve(),dbc],input=json.dumps(request),text=True,capture_output=True,check=False)
  (args.evidence/'process.log').write_text(child.stdout+child.stderr+f'\nEXIT {child.returncode}\n');child.check_returncode()
  actual=json.loads(target.read_text());left=[decoded(value) for value in expected];right=[decoded(value) for value in actual]
  (args.evidence/'source-decoded.json').write_text(json.dumps(left)+'\n');(args.evidence/'native-decoded.json').write_text(json.dumps(right)+'\n')
  try:compare(left,right)
  except AssertionError as error:(args.evidence/'comparison-failure.txt').write_text(str(error)+'\n');raise
  files=[ROOT/'opendbc_repo/opendbc/car/interfaces.py',ROOT/'opendbc_repo/opendbc/dbc/comma_body.dbc',ROOT/'openpilot/selfdrive/car/car_specific.py']
  files+=list((ROOT/'opendbc_repo/opendbc/car/body').glob('*.py'))+list((ROOT/'opendbc_repo/opendbc/car/mock').glob('*.py'))
  result=dict(status='pass',cases=len(request),steps=sum(len(case['steps']) for case in request),runtime_python=False,
      observable='complete Body/Mock CarParams, CarState and actuator schemas; exact CAN/controller cadence/counters/checksums; original Mock GPS selection',
      scope='brand runtime policy with owned CAN/GPS/control fixtures; common startup assets and socket composition validated separately',
      binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),source_sha256={str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in files})
  (args.evidence/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))


if __name__=='__main__':main()

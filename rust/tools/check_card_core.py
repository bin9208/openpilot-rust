import argparse
from contextlib import redirect_stderr,redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess
from card_core_source import ROOT,decoded,trace
from check_card_vehicle import compare
from openpilot.cereal import car,log


def event(name: str,now: int,values: dict,valid=True) -> list[int]:
  message=log.Event.new_message(valid=valid,logMonoTime=now)
  if isinstance(values,list): message.init(name,len(values)); setattr(message,name,values)
  else: message.init(name); getattr(message,name).from_dict(values)
  return list(message.to_bytes())


def cases() -> list[dict]:
  result=[]
  for passive,dashcam,replay,controller in ((False,False,False,True),(False,False,True,True),(True,False,False,True),(True,True,False,True),(True,False,False,False)):
    cp=car.CarParams.new_message(brand='hyundai',carFingerprint='HYUNDAI_IONIQ_5',passive=passive,dashcamOnly=dashcam,openpilotLongitudinalControl=True)
    steps=[]
    count=5011 if len(result)==0 else 225
    for index in range(count):
      now=1_000_000_000+index*10_000_000
      state=car.CarState.new_message(canValid=index%5!=0,vEgo=float(index%50),buttonEvents=[dict(type='mainCruise',pressed=True)] if index==0 else [],leftLaneLine=20,rightLaneLine=10)
      messages=[]
      if index%6!=5: messages.append(event('carControl',now,dict(enabled=index>=250 and index%92>=46,actuators=dict(accel=1.,torque=.25)),valid=index%13!=0))
      if index%17==0: messages.append(event('onroadEvents',now,[] if index%34==0 else [dict(name='selfdriveInitializing')]))
      if index%3==0: messages.append(event('modelV2',now,{},valid=index%9!=0))
      if index%4==0: messages.append(event('radarState',now,{},valid=index%8!=0))
      if index in (0,30,45,80):
        payload=dict(type='xiaogeVision',version=1,lane=dict(leftLine=1,rightLine=-1,valid=True,receivedMonoTimeNanos=now),blindspot=dict(left=False,right=True,valid=True,receivedMonoTimeNanos=now))
        raw=b'{' if index==45 else json.dumps(payload).encode()
        message=log.Event.new_message(logMonoTime=now); message.customReservedRawData0=raw; messages.append(list(message.to_bytes()))
      can=[] if index%4==0 else [event('can',now-1,[dict(address=123,dat=bytes([1,2,3]),src=4)]),event('can',now-2,[])]
      settings={'IsMetric':'1' if index%20<10 else '0','ExperimentalMode':'1','OpenpilotEnabledToggle':'0'} if index%100==0 else {}
      steps.append(dict(now=now,can=can,messages=messages,state=list(state.to_bytes()),accel=float(index%20)/4,settings=settings,remaining=-.005))
    result.append(dict(params=list(cp.to_bytes()),has_controller=controller,replay=replay,steps=steps))
  cp=car.CarParams.new_message(brand='hyundai',passive=False)
  step=result[1]['steps'][0].copy(); step['can']=[]; step['messages']=[event('carControl',step['now'],dict(enabled=True)),event('onroadEvents',step['now'],[])]
  result.append(dict(params=list(cp.to_bytes()),has_controller=True,replay=True,steps=[step]))
  return result


def main() -> None:
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--evidence',type=Path,required=True)
  args=parser.parse_args();args.evidence.mkdir(parents=True,exist_ok=True)
  request=cases();source_log=io.StringIO()
  with redirect_stdout(source_log),redirect_stderr(source_log): expected=[trace(case) for case in request]
  (args.evidence/'input.json').write_text(json.dumps(request)+'\n');(args.evidence/'source.json').write_text(json.dumps(expected)+'\n');(args.evidence/'source.log').write_text(source_log.getvalue() or 'no extra source diagnostics\n')
  target=args.evidence/'native.json';child=subprocess.run([args.binary.resolve(),target.resolve()],input=json.dumps(request),text=True,capture_output=True,check=False)
  (args.evidence/'process.log').write_text(child.stdout+child.stderr+f'\nEXIT {child.returncode}\n');child.check_returncode()
  actual=json.loads(target.read_text());left=[decoded(frames) for frames in expected];right=[decoded(frames) for frames in actual]
  (args.evidence/'source-decoded.json').write_text(json.dumps(left)+'\n');(args.evidence/'native-decoded.json').write_text(json.dumps(right)+'\n')
  try:compare(left,right)
  except AssertionError as error:
    (args.evidence/'comparison-failure.txt').write_text(str(error)+'\n');raise
  result=dict(status='pass',cases=len(request),steps=sum(len(case['steps']) for case in request),runtime_python=False,
      binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
      observable='complete publications/Cereal, validity/order/cadence, prior actuator output, init/apply/model/radar/replay gates, modified CI state, timeout/diagnostic/toggle/Params transitions',
      boundaries='unchanged source Car loop methods + original SubMaster; owned CI and cruise-tail fixture, fixed source monotonic/CPU clocks, no physicalCAN',
      source_sha256={str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in [ROOT/'openpilot/selfdrive/car/card.py',ROOT/'openpilot/cereal/messaging/__init__.py',ROOT/'openpilot/selfdrive/pandad/pandad_api_impl.py']})
  (args.evidence/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))


if __name__=='__main__':main()

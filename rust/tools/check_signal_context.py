"""Actual prepare runtime camera/calibration/pose context, fixed base-I/O inputs."""
import argparse,ast,dataclasses,itertools,json,math,subprocess,sys
from pathlib import Path
from types import ModuleType,SimpleNamespace as NS
import numpy as np
from signal_source import source,compare
from check_signal_geometry import load as load_rotation

def main():
  p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
  load_rotation();camera=ModuleType('openpilot.common.transformations.camera');sys.modules[camera.__name__]=camera
  code=source('openpilot/common/transformations/camera.py').split('# device/mesh')[0].replace('import openpilot.common.transformations.orientation as orient','')
  exec(compile(code,'e8e152db:camera-config','exec'),camera.__dict__)
  mode=ModuleType('openpilot.selfdrive.carrot.signal_mode');mode.signal_mode=lambda:1;sys.modules[mode.__name__]=mode
  class Base:
    def read(self,sm,now=None):return None,dict(now=20. if now is None else now,enabled=True,valid=True,drive=True)
  node=next(n for n in ast.parse(source('openpilot/selfdrive/carrot/signal_stop_prepare_runtime.py')).body if isinstance(n,ast.ClassDef) and n.name=='SignalPrepareRuntime')
  scope={'SignalAssistRuntime':Base,'np':np,'math':math,'time':NS(monotonic=lambda:20.),'housing_range':None,'housing_alignment':None,'night_lamp_support':None}
  exec(compile(ast.Module(body=[node],type_ignores=[]),'e8e152db:SignalPrepareRuntime','exec'),scope)
  cases=[];expected=[]
  for rpy,omega,std,variant in itertools.product([[0.,0.,0.],[.02,.025,.05],[-.02,-.01,-.05]],[[0.,0.,0.],[0.,0.,math.radians(1.5)],[0.,0.,math.nextafter(math.radians(1.5),math.inf)]],[[.001,.002,.003],[math.radians(2.),0.,0.]],range(32)):
    fresh=lambda:dict(seen=True,valid=True,alive=True,receive_time=20.)
    c=dict(calibration=fresh(),camera=fresh(),device=fresh(),pose=fresh(),calibrated=True,device_type='tici',sensor='os04c10',rpy=rpy,pose_timestamp_ns=20_000_000_000,inputs_ok=True,posenet_ok=True,sensors_ok=True,angular_valid=True,orientation_valid=True,angular=omega,angular_std=std)
    if variant==1:c['calibration']['seen']=False
    if variant==2:c['calibration']['valid']=False
    if variant==3:c['camera']['receive_time']-=.250001
    if variant==4:c['device']['receive_time']-=2.000001
    if variant==5:c['calibrated']=False
    if variant==6:c['device_type']='pc'
    if variant==7:c['sensor']='ar0231'
    if variant==8:c['rpy']=[0.,0.]
    if variant==9:c['pose']['seen']=False
    if variant==10:c['pose']['receive_time']-=.200001
    if variant==11:c['pose_timestamp_ns']-=200_001_000
    if variant==12:c['inputs_ok']=False
    if variant==13:c['posenet_ok']=False
    if variant==14:c['sensors_ok']=False
    if variant==15:c['angular_valid']=False
    if variant==16:c['orientation_valid']=False
    if variant==17:c['angular_std']=[-.001,.001,.001]
    if variant==18:c['angular']=[0.,0.]
    if variant==19:c['angular_std']=[0.,0.]
    if variant==20:c['calibration']['receive_time']+=.000001
    if variant==21:c['pose_timestamp_ns']+=1
    if variant==22:c['camera']['alive']=False
    if variant==23:c['device_type']='mici'
    if variant==24:c['device_type']='tizi'
    if variant==25:c['device']['receive_time']+=.000001
    if variant==26:c['calibration']['receive_time']-=2.
    if variant==27:c['camera']['receive_time']-=.25
    if variant==28:c['pose']['receive_time']-=.2;c['pose_timestamp_ns']-=200_000_000
    if variant==29:c['pose']['alive']=False
    if variant==30:c['camera']['receive_time']-=.250000000001
    if variant==31:c['device_type']='unknown';c['sensor']='ox03c10'
    class SM(dict):pass
    extra=SM(liveCalibration=NS(calStatus='calibrated' if c['calibrated'] else 'uncalibrated',rpyCalib=c['rpy']),roadCameraState=NS(sensor=c['sensor']),deviceState=NS(deviceType=c['device_type']))
    extra.update=lambda timeout:None
    angular=dict(zip(['x','y','z'],c['angular']));angular.update(zip(['xStd','yStd','zStd'],c['angular_std']));angular['valid']=c['angular_valid']
    sm=SM(livePose=NS(timestamp=c['pose_timestamp_ns'],inputsOK=c['inputs_ok'],posenetOK=c['posenet_ok'],sensorsOK=c['sensors_ok'],angularVelocityDevice=NS(**angular),orientationNED=NS(valid=c['orientation_valid'])),modelV2=NS(roadEdges=[],roadEdgeStds=[],laneLines=[],laneLineProbs=[],laneLineStds=[]))
    for field,key in [('seen','seen'),('valid','valid'),('alive','alive'),('recv_time','receive_time')]:
      setattr(extra,field,{name:c[component][key] for name,component in [('liveCalibration','calibration'),('roadCameraState','camera'),('deviceState','device')]});setattr(sm,field,{'livePose':c['pose'][key]})
    native=scope['SignalPrepareRuntime'].__new__(scope['SignalPrepareRuntime']);native.enabled=True;native.extra=extra;native.mode=1;native.mode_since=-math.inf
    _,context=native.read(sm,now=20.)
    if context['valid']:
      import inspect
      captured=inspect.getclosurevars(context['geometry']).nonlocals
      expected.append({'focal':captured['config'].fcam.focal_length,'rpy':captured['rpy'],'turning':bool(context['turning'])})
    else:expected.append(None)
    cases.append({'now':20.,'context':c})
  (a.output/'input.json').write_text(json.dumps(cases));(a.output/'expected.json').write_text(json.dumps(expected))
  if a.binary:
    raw=subprocess.check_output([str(a.binary.resolve())],input=json.dumps(cases).encode());(a.output/'actual.json').write_bytes(raw);(a.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'cases':len(cases),'valid':sum(v is not None for v in expected),'turning':sum(v['turning'] for v in expected if v),'native_compared':bool(a.binary)}))
if __name__=='__main__':main()

"""Actual bounded endpoint reader/cache/mode/vehicle-context source oracle."""
import argparse,ast,json,math,subprocess,tempfile
from pathlib import Path
from types import SimpleNamespace as NS
from signal_source import source,compare
SERVICES=['carState','modelV2','radarState','selfdriveState','carControl']
def clean(v):
  if isinstance(v,float) and not math.isfinite(v):return None
  if isinstance(v,dict):return {k:clean(x) for k,x in v.items()}
  if isinstance(v,(tuple,list)):return [clean(x) for x in v]
  return v

def main():
  p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
  node=next(n for n in ast.parse(source('openpilot/selfdrive/carrot/signal_assist_runtime.py')).body if isinstance(n,ast.ClassDef) and n.name=='SignalAssistRuntime')
  setting=[False];now=[20.];scope={'Path':Path,'json':json,'time':NS(monotonic=lambda:now[0]),'MAX_BYTES':32768,'ENABLE_PATH':Path('/unused/enabled'),'OBSERVATION_PATH':Path('/unused/obs'),'requested':lambda path:setting[0],'SignalAssist':lambda **kw:object()}
  exec(compile(ast.Module(body=[node],type_ignores=[]),'e8e152db:SignalAssistRuntime','exec'),scope)
  cases=[];expected=[]
  with tempfile.TemporaryDirectory(dir=a.output) as scratch:
    endpoint=Path(scratch)/'observation.json'
    for variant in range(26):
      initial=variant%2==0;setting[0]=initial;runtime=scope['SignalAssistRuntime'](Path(scratch)/'enabled',endpoint);frames=[];values=[]
      for index in range(20):
        now[0]=20.+index*.1;setting[0]=not(variant in [11,24] and 5<=index<12)
        services=[{'seen':True,'valid':True,'alive':True,'receive_time':now[0]} for _ in SERVICES]
        if index>6:
          if variant==2:services[1]['receive_time']-=.26
          if variant==3:services[2]['receive_time']+=.01
          if variant==4:services[0]['seen']=False
          if variant==5:services[3]['valid']=False
          if variant==6:services[4]['alive']=False
        vehicle={'services':services,'can_valid':variant!=7 or index<6,'selfdrive_enabled':variant!=8 or index<6,'longitudinal_active':variant!=9 or index<6,'drive':variant!=10 or index<6}
        obj={'version':1,'stream':'road','size':[1344,760],'timestamp':20. if variant in [1,24] else now[0],'frame_id':index,'session':'α' if variant==25 else 'source','tracks':[]}
        if variant==15:obj=[]
        if variant==16:obj['version']=True
        if variant==17:obj.update(version=1.,size=[1344.,760.])
        if variant==18:obj.pop('timestamp')
        if variant==19:obj['timestamp']=True
        if variant==20:obj['timestamp']='invalid'
        if variant==21:obj=None
        if variant==22:obj['stream']='wideRoad'
        if variant==23:obj['size']=[1928,1208]
        raw=None if variant==12 else '{' if variant==13 else ' '*32769 if variant==14 else json.dumps(obj)
        if raw is None:endpoint.unlink(missing_ok=True)
        else:endpoint.write_text(raw)
        class SM(dict):pass
        sm=SM(carState=NS(canValid=vehicle['can_valid'],gearShifter='drive' if vehicle['drive'] else 'park'),selfdriveState=NS(enabled=vehicle['selfdrive_enabled']),carControl=NS(longActive=vehicle['longitudinal_active']))
        for key,field in [('seen','seen'),('valid','valid'),('alive','alive'),('recv_time','receive_time')]:setattr(sm,key,{name:s[field] for name,s in zip(SERVICES,services)})
        observation,context=runtime.read(sm,now=now[0]);state={'enabled':runtime.enabled,'last_mode_check':runtime.last_mode_check,'enabled_since':runtime.enabled_since,'bytes':list(runtime._observation_bytes) if runtime._observation_bytes is not None else None,'observation':runtime._observation}
        frames.append({'now':now[0],'requested':setting[0],'raw':raw,'vehicle':vehicle});values.append(json.loads(json.dumps(clean({'observation':observation,'context':context,'state':state}))))
      cases.append({'initial_enabled':initial,'frames':frames});expected.append(values)
  (a.output/'input.json').write_text(json.dumps(cases));(a.output/'expected.json').write_text(json.dumps(expected))
  if a.binary:
    raw=subprocess.check_output([str(a.binary.resolve())],input=json.dumps(cases).encode());(a.output/'actual.json').write_bytes(raw);(a.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'cases':len(cases),'reads':sum(len(c['frames']) for c in cases),'native_compared':bool(a.binary)}))
if __name__=='__main__':main()

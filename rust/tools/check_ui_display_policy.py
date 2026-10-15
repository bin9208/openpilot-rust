"""Pinned-source display point selection and blindspot provenance, independent of control."""
import argparse
import ast
from copy import deepcopy
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import random
import subprocess
import sys
import types

PIN='e8e152dbcfd9574a1791ed12026d1880e87689b3'
parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--runner',nargs=argparse.REMAINDER,default=[]);args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
modules=[];hashes={}
for name,path in [('xiaoge','openpilot/selfdrive/carrot/xiaoge/xiaoge_vision.py'),('radar','openpilot/selfdrive/ui/radar_display.py'),('vision','openpilot/selfdrive/ui/vision_status.py')]:
  qualified='openpilot.selfdrive.carrot.xiaoge.xiaoge_vision' if name=='xiaoge' else name
  module=types.ModuleType(qualified);sys.modules[qualified]=module
  source=subprocess.check_output(['git','show',PIN+':'+path]);hashes[path]=hashlib.sha256(source).hexdigest();exec(compile(source,path,'exec'),module.__dict__);modules.append(module)
_,radar,vision=modules
class SM(dict):pass
cases=[];expected=[]
base={'supported':True,'seen':True,'alive':True,'valid':True,'receive_frame':10,'receive_time':10.,'publication_ns':10_000_000_000,'errors':False,'points':[{'track_id':1,'measured':True,'front_radar':True,'d_rel':25.,'y_rel':-1.5,'v_lead':8.,'yv_rel':0.5}]}
def append_radar(value,show=3,now=10.,started=10):
  sm=SM();sm['liveTracks']=types.SimpleNamespace(hasDisplayData=value['supported'],errors=types.SimpleNamespace(to_dict=lambda:{'canError':value['errors']}),displayPoints=[types.SimpleNamespace(trackId=p['track_id'],measured=p['measured'],radarSource='frontRadar' if p['front_radar'] else 'scc',dRel=p['d_rel'],yRel=p['y_rel'],vLead=p['v_lead'],yvRel=p['yv_rel']) for p in value['points']])
  for field,key in [('seen','seen'),('alive','alive'),('valid','valid'),('recv_frame','receive_frame'),('recv_time','receive_time'),('logMonoTime','publication_ns')]:setattr(sm,field,{'liveTracks':value[key]})
  result=radar.select_radar_display_points(sm,show,now=now,started_frame=started)
  mapped=None if result is None else [{'d_rel':p.dRel,'y_rel':p.yRel,'v_lead_k':p.vLeadK,'v_lat':p.vLat,'radar':p.radar,'model_prob':p.modelProb} for p in result]
  cases.append({'kind':'radar','input':deepcopy(value),'show':show,'now':now,'started_frame':started});expected.append(mapped)
for show in [-1,0,1,2,3,4]:
 for age in [-1e-9,0.,0.199999999,0.2,0.200000001,1.]:
  for field,values in [('supported',[False,True]),('seen',[False,True]),('alive',[False,True]),('valid',[False,True]),('errors',[False,True]),('receive_frame',[9,10,11]),('receive_time',[0.,9.8,9.799999999,10.,10.000000001]),('publication_ns',[0,9_800_000_000,9_799_999_999,10_000_000_000,10_000_000_001])]:
   for value in values:
    data=deepcopy(base);data[field]=value;append_radar(data,show,10.+age)
for key,values in [('d_rel',[-1.,0.,0.001,245.9375,245.937501]),('y_rel',[-30.720001,-30.72,0.,30.72,30.720001]),('track_id',[0,2**31-1,2**31,2**63]),('measured',[False,True]),('front_radar',[False,True])]:
 for value in values:
  data=deepcopy(base);data['points'][0][key]=value;append_radar(data)
for count in [0,1,2,19,20,21]:
 for duplicate in [False,True]:
  data=deepcopy(base);data['points']=[dict(base['points'][0],track_id=0 if duplicate else i) for i in range(count)];append_radar(data)
rng=random.Random(338)
for i in range(512):
 data=deepcopy(base);data['points']=[dict(base['points'][0],track_id=rng.randrange(30),d_rel=rng.uniform(-20,280),y_rel=rng.uniform(-35,35),measured=rng.choice([True,False]),front_radar=rng.choice([True,False])) for _ in range(rng.randrange(24))];append_radar(data)
radar_count=len(cases)
for index in range(1024):
 valid=bool(index&1);split=bool(index&2);merged=[bool(index&4),bool(index&8)];sources=[[bool(index&16),bool(index&32)],[bool(index&64),bool(index&128)]]
 payload={'type':'xiaogeBlindspotSources','version':1,'left':{'oem':bool(index&4),'vision':bool(index&16)},'right':{'oem':bool(index&8),'vision':bool(index&32)},'receivedMonoTimeNanos':[0,10_000_000_000,8_500_000_000,8_499_999_999][index%4]}
 if index%17==0:payload['left']['vision']=1
 if index%19==0:payload['receivedMonoTimeNanos']=-1
 raw=json.dumps(payload).encode();fresh=False
 try:
  packet=vision.parse_blindspot_source_packet(raw);accepted=True;fresh=0<=10_000_000_000-packet.received_nanos<=1_500_000_000
 except (ValueError,TypeError):packet=None;accepted=False
 if not valid or not fresh:packet=None
 car=types.SimpleNamespace(blindspotSplitSourcesValid=split,leftBlindspot=merged[0],rightBlindspot=merged[1],leftBlindspotOem=sources[0][0],leftBlindspotOnnx=sources[0][1],rightBlindspotOem=sources[1][0],rightBlindspotOnnx=sources[1][1])
 sides=[vision.blindspot_sources(car,side,packet) for side in ['left','right']]
 colors=[(255,128,128,190) if o and v else (64,156,255,165) if v else (255,215,0,150) for o,v in sides]
 tints=[r|(g<<8)|(b<<16)|(a<<24) for r,g,b,a in colors]
 cases.append({'kind':'blindspot','payload':list(raw),'now':10_000_000_000,'valid':valid,'split':split,'merged':merged,'sources':sources});expected.append({'accepted':accepted,'fresh':fresh,'sides':[list(s) for s in sides],'tints':tints})
blindspot_count=len(cases)-radar_count
DISPLAY_PIN='8cf5570f6962bd9cb17ec384bbfa12a3a167a5a8'
source=subprocess.check_output(['git','show',DISPLAY_PIN+':openpilot/selfdrive/ui/signal_status.py'])
hashes['openpilot/selfdrive/ui/signal_status.py']=hashlib.sha256(source).hexdigest()
scope={'signal_display_status':lambda:(0,False,False),'display_requested':lambda:True};tree=ast.parse(source);exec(compile(ast.Module(body=[n for n in tree.body if isinstance(n,ast.FunctionDef)],type_ignores=[]),'signal_status.py','exec'),scope)
for valid in [False,True]:
 for alive in [False,True]:
  for suspended in [False,True]:
   for color in [0,1,2,3,4,255]:
    for action in [0,1,2,3,4,255]:
     sm=SM(longitudinalPlan=types.SimpleNamespace(signalDetectionSuspended=suspended,signalObservationColor=color,signalAssistAction=action));sm.valid={'longitudinalPlan':valid};sm.alive={'longitudinalPlan':alive}
     scope['signal_display_status']=lambda:(color if color in (1,2,3) else 0,False,True)
     c,a=scope['signal_assist_status'](sm)
     cases.append({'kind':'signal','valid':valid,'alive':alive,'suspended':suspended,'color':color,'action':action});expected.append({'paused':scope['signal_detection_paused'](sm),'color':c,'action':a})
signal_count=len(cases)-radar_count-blindspot_count
for valid in [False,True]:
 for alive in [False,True]:
  for requested in [False,True]:
   for online in [False,True]:
    sm=SM();sm.valid={'longitudinalPlan':valid};sm.alive={'longitudinalPlan':alive}
    scope['display_requested']=lambda:requested
    scope['signal_display_status']=lambda:(0,False,online)
    cases.append(dict(kind='visibility',valid=valid,alive=alive,requested=requested,online=online))
    expected.append(scope['signal_display_visible'](sm))

from signal_source import source as signal_source
import tempfile
summary_scope={}
display_source=signal_source('openpilot/selfdrive/carrot/signal_display.py')
hashes['openpilot/selfdrive/carrot/signal_display.py']=hashlib.sha256(display_source.encode()).hexdigest()
exec(compile(display_source,DISPLAY_PIN+':signal_display.py','exec'),summary_scope)
packet=lambda stamp,frame,raw,session='a':dict(version=1,stream='road',size=[1344,760],session=session,timestamp=stamp,frame_id=frame,tracks=[dict(id=i+1,box=[100+i*50,200,130+i*50,250],raw=c,state=c,age=0) for i,c in enumerate(raw)])
steps=[]
def step(now,record,requested=True):steps.append(dict(now=now,record=record,requested=requested))
red=packet(10.,100,['red'])
step(10.,red);step(10.02,red);step(10.36,red)
step(10.5,packet(10.5,110,['unknown']));step(11.99,packet(11.99,140,[]));step(12.01,packet(12.01,141,[]))
step(12.1,packet(12.1,142,['green'],'b'));step(12.2,packet(12.2,143,['red','green'],'b'))
step(12.3,packet(12.15,144,['unknown'],'b'));step(12.4,packet(12.4,145,['green'],'c'))
step(12.5,None);step(12.6,packet(12.6,146,['red']));step(12.7,packet(12.6,146,['red']),False)
step(12.8,packet(12.8,147,['green']));step(13.,packet(13.,148,['unknown']));step(14.8,packet(13.,148,['unknown']));step(14.81,packet(13.,148,['unknown']))
cluster_path='openpilot/selfdrive/carrot/cluster/cluster_renderer.py'
cluster_source=subprocess.check_output(['git','show',DISPLAY_PIN+':'+cluster_path])
hashes[cluster_path]=hashlib.sha256(cluster_source).hexdigest()
cluster_class=next(n for n in ast.parse(cluster_source).body if isinstance(n,ast.ClassDef) and n.name=='ClusterUiRenderer')
ring_method=next(n for n in cluster_class.body if isinstance(n,ast.FunctionDef) and n.name=='_draw_model_traffic_state')
ring_calls=[]
ring_scope={'rl':types.SimpleNamespace(draw_circle_lines=lambda *a:ring_calls.append(a)), 'rl_color':lambda rgb:rgb, 'SPEED_MODEL_TRAFFIC_CENTER_X':0., 'SPEED_MODEL_TRAFFIC_CENTER_Y':0., 'SPEED_MODEL_TRAFFIC_ICON_SIZE':10.}
exec(compile(ast.Module(body=[ring_method],type_ignores=[]),DISPLAY_PIN+':'+cluster_path,'exec'),ring_scope)
display_module=types.ModuleType('openpilot.selfdrive.carrot.signal_display')
display_module.display_requested=lambda:True
sys.modules[display_module.__name__]=display_module
with tempfile.TemporaryDirectory() as directory:
 path=Path(directory)/'display.json';reader=summary_scope['SignalStatusReader'](path);rows=[]
 for item in steps:
  summary_scope['display_requested']=lambda:item['requested']
  path.write_text(json.dumps(item['record']))
  status=reader.read(item['now']);display_module.signal_display_status=lambda:status
  ring_calls.clear();ring_scope['_draw_model_traffic_state'](None,0)
  assert len(ring_calls)==3 and [r[2] for r in ring_calls]==[7.,8.,9.]
  rgb=ring_calls[0][3]
  rows.append(dict(status=list(status),rgb=list(rgb)))
cases.append(dict(kind='summary',steps=steps));expected.append(rows)

source=subprocess.check_output(['git','show',PIN+':openpilot/system/ui/lib/overlay_cache.py']);tree=ast.parse(source)
clock_time=types.SimpleNamespace(monotonic=lambda:now)
scope={'time':clock_time};exec(compile(ast.Module(body=[n for n in tree.body if isinstance(n,ast.ClassDef) and n.name=='DisplayClock'],type_ignores=[]),'overlay_cache.py','exec'),scope)
for retention in [0.,0.05,0.2,0.8,0.85,1.]:
 times=[0.,0.,0.025,0.05,0.1,0.2,0.5,1.,0.99,2.]+[2.+i/60 for i in range(90)]
 clock=scope['DisplayClock']();rows=[]
 for now in times:rows.append([clock.tick(),clock.retention(retention)])
 cases.append({'kind':'clock','times':times,'retention':retention});expected.append(rows)
(args.output/'input.json').write_text(json.dumps(cases));(args.output/'source.json').write_text(json.dumps(expected))
result=subprocess.run([*args.runner,str(args.binary)],input=json.dumps(cases),text=True,capture_output=True);(args.output/'native.stderr').write_text(result.stderr);result.check_returncode();native=json.loads(result.stdout);(args.output/'native.json').write_text(result.stdout)
differences=[{'index':i,'case':case,'source':a,'native':b} for i,(case,a,b) in enumerate(zip(cases,expected,native,strict=True)) if a!=b]
(args.output/'result.json').write_text(json.dumps({'source_commit':PIN,'display_source_commit':DISPLAY_PIN,'summary_steps':len(steps),'hashes':hashes,'radar_cases':radar_count,'blindspot_cases':blindspot_count,'signal_cases':signal_count,'clock_steps':600,'differences':differences},indent=2));assert not differences,differences[:3]
print(f'PASS {radar_count} radar, {blindspot_count} blindspot, {signal_count} signal,16visibility,{len(steps)} summary and600clock steps')

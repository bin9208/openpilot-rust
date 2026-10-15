"""Final Carrot + both MPC owners with active source start/prepare decisions.

I/O, clocks and geometry callbacks are explicit fixtures. Full final owners and
signal state machines run unchanged; geometry and producer have separate gates.
"""
import argparse,dataclasses,hashlib,json,math,subprocess,sys,types
from pathlib import Path
import plannerd_owner_loader as loader
import plannerd_owner_fixtures as fixtures
import plannerd_owner_snapshot as snapshots
from signal_source import source
from plannerd_owner_source import difference

REQUESTS=[]
WIRE_REQUESTS=[]
OWNER=None

def register(path):
  name=path.removesuffix('.py').replace('/','.');module=types.ModuleType(name);module.__file__='e8e152db:'+path;sys.modules[name]=module
  exec(compile(source(path),module.__file__,'exec'),module.__dict__);return module

def sanitize(value):
  if isinstance(value,float) and not math.isfinite(value):return None
  if isinstance(value,dict):return {str(k):sanitize(v) for k,v in value.items()}
  if isinstance(value,(list,tuple)):return [sanitize(v) for v in value]
  return value

def signals(index,now,suspended):
  context={'now':now,'enabled':True,'valid':True,'drive':True};start=dict(context,enabled=index<120 or index>=240);prepare=dict(context,enabled=120<=index<240)
  raw='red' if index<40 or index>=80 else 'green'
  packet={'version':1,'stream':'road','size':[1344,760],'timestamp':now,'frame_id':index,'session':'active-owner','tracks':[{'id':1,'box':[620.,120.,700.,160.],'raw':raw,'seen_red':True}]}
  prep={'version':1,'stream':'road','size':[1344,760],'timestamp':now,'frame_id':index,'session':'active-owner','tracks':[{'id':1,'box':[620.,200.,660.,210.],'age':0.,'observations':index+3,'state':'red','evidence':{'raw':'red','source':'detected','quality':.9,'red_score':.8,'green_score':0.,'core_diameter':10.}}]}
  if index in range(260,275):packet['timestamp']-=.7
  if index in range(290,300):start['valid']=False
  return {'suspended':suspended,'start':start,'start_observation':packet,'prepare':prepare,'prepare_observation':prep,'prepare_actuate':True,'turning':False,'geometry':{'range':[45.,85.],'alignment':[-.5,.5],'night_range':None}}

def main():
  global OWNER
  parser=argparse.ArgumentParser(add_help=False);parser.add_argument('--binary',type=Path);parser.add_argument('--wire',action='store_true');args,remaining=parser.parse_known_args()
  original_init=loader.SourceOwners.__init__;original_messages=fixtures.messages;original_parameters=fixtures.parameters;original_snapshot=snapshots.frame
  def parameters():
    values=original_parameters();values.update(TrafficLightDetectMode='2',MyDrivingModeAuto='0');return values
  def messages(factory,index,now):
    packets=original_messages(factory,index,now);car=packets['carState'].carState;speed=10. if 120<=index<240 else 0.
    car.vEgo=car.vEgoCluster=speed;car.aEgo=0.;car.standstill=speed==0.;car.gasPressed=index==350;car.brakePressed=False;car.leftBlinker=car.rightBlinker=False;car.softHoldActive=0;car.gearShifter='drive';car.canValid=True
    control=packets['controlsState'].controlsState;control.longControlState='pid';control.forceDecel=False;control.curvature=control.desiredCurvature=0.
    packets['carControl'].carControl.longActive=True;packets['selfdriveState'].selfdriveState.enabled=True;packets['selfdriveState'].selfdriveState.experimentalMode=False
    fixtures.model(packets['modelV2'].modelV2,1000,20.);packets['modelV2'].modelV2.meta.laneChangeState='off';packets['modelV2'].modelV2.meta.desire='none'
    radar=packets['radarState'].radarState;radar.leadOne.status=radar.leadTwo.status=radar.leadCutInRisk.status=False
    # Moving preparation permits far MPC leads, then immediately yields to
    # either nearby lead and soft hold without removing those MPC obstacles.
    if 130 <= index < 220:
      lead = radar.leadOne if index < 175 else radar.leadTwo
      lead.status = True
      lead.dRel = 80. if index < 155 or 175 <= index < 200 else 50.
    if 225 <= index < 240:
      car.softHoldActive = 1
    nav=packets['carrotMan'].carrotMan;nav.vTurnSpeed=0;nav.trafficState=0;nav.atcType=''
    packet=factory('carrotNavi');packet.logMonoTime=int(now*1e9);packet.valid=True;navi=packet.carrotNavi;navi.schemaVersion=1;navi.connected=True;navi.sessionId='owner-route';navi.laneCurrent.meta.present=True;navi.laneCurrent.meta.receivedMonoTimeNanos=int(now*1e9);navi.laneCurrent.roadCategoryValid=True;navi.laneCurrent.roadCategory=0 if 320<=index<340 else 2
    packets['carrotNavi']=packet;return packets
  def init(self,*a,**kw):
    global OWNER
    original_init(self,*a,**kw);OWNER=self
    self.long.time=types.SimpleNamespace(monotonic=lambda:self.now)
    if args.wire:
      original_submaster=self.messaging.SubMaster
      class CapturedSubMaster(original_submaster):
        def __init__(this,*a,**kw):
          super().__init__(*a,**kw);self.sm=this
      self.messaging.SubMaster=CapturedSubMaster
    modules={}
    for part in ['signal_assist','signal_mode','signal_assist_runtime','signal_start_guard','signal_stop_prepare','signal_stop_prepare_runtime','signal_road_policy','carrot_functions']:
      modules[part]=register('openpilot/selfdrive/carrot/'+part+'.py')
    modules['signal_road_policy'].time=types.SimpleNamespace(monotonic=lambda:self.now)
    final=modules['carrot_functions'];final.time=types.SimpleNamespace(time=lambda:self.wall,monotonic=lambda:self.now);actual=final.CarrotPlanner
    class Carrot(actual):
      def __init__(this):
        super().__init__(signal_assist=False,start_guard=modules['signal_start_guard'].SignalStartGuard(),stop_prepare=modules['signal_stop_prepare'].SignalStopPrepare());this.signal_assist=None
      def update(this,sm,*a,**kw):
        index=round((self.now-100.)/.05);suspended=modules['signal_road_policy'].signal_detection_suspended(sm);request=signals(index,self.now,suspended);REQUESTS.append(request)
        context=dict(request['prepare'],actuate=request['prepare_actuate'],turning=request['turning'],geometry=lambda box:(45.,85.),alignment=lambda box,distance:(-.5,.5))
        return super().update(sm,*a,**kw,start_context=request['start'],start_observation=request['start_observation'],prepare_context=context,prepare_observation=request['prepare_observation'])
    final.CarrotPlanner=Carrot;self.carrot=final
  def snapshot(long,lat,carrot,*a):
    value=original_snapshot(long,lat,carrot,*a);value['signal_owner']=sanitize({'state':{'start':vars(carrot.start_guard),'prepare':vars(carrot.stop_prepare)},'start':dataclasses.asdict(carrot.start_decision),'prepare':dataclasses.asdict(carrot.prepare_decision),'control_active':carrot.prepare_control_active,'suspended':carrot.signal_detection_suspended,'color':carrot.signal_observation_color,'green':carrot.signal_green_active,'constraint':{'time':long._signal_prepare_time,'previous':long._signal_prepare_previous,'cap':long._signal_prepare_accel.cap}})
    if args.wire:
      index=len(WIRE_REQUESTS);sm=OWNER.sm
      reasons=['inactive','active','notRadarLead','selectionPending','selectionUnstable','trackMissing','trackUnmeasured','nonFinite','invalidDistance','distanceDiscontinuity','velocityDiscontinuity','radarStateInvalid','liveTracksInvalid','selectionStale']
      kwargs=dict(live_tracks_mono_time=int(OWNER.now*1e9)-5000000,fast_lead_mask=index%4,fast_lead_track_id=42 if index%2 else -1,planning_trigger='liveTracks' if index%2 else 'modelV2',fast_lead_reason=reasons[index%len(reasons)])
      published=[];saved=long.mpc.solve_time;long.mpc.solve_time=0.
      try:
        long.publish(sm,types.SimpleNamespace(send=lambda name,msg:published.append(msg.to_dict())),carrot,**kwargs)
      finally:
        long.mpc.solve_time=saved
      assert len(published)==1
      value['wire']=published[0]
      WIRE_REQUESTS.append(dict(now_ns=published[0]['logMonoTime'],valid=published[0]['valid'],model_ns=sm.logMonoTime['modelV2'],radar_ns=sm.logMonoTime['radarState'],live_ns=kwargs['live_tracks_mono_time'],fast_mask=kwargs['fast_lead_mask'],fast_id=kwargs['fast_lead_track_id'],trigger=kwargs['planning_trigger'],fast_reason=kwargs['fast_lead_reason'],lead_status=bool(sm['radarState'].leadOne.status)))
    return value
  loader.SourceOwners.__init__=init;fixtures.messages=messages;fixtures.parameters=parameters;snapshots.frame=snapshot;fixtures.SERVICES.append('carrotNavi')
  sys.argv=[sys.argv[0],*remaining]+([] if '--source-only' in remaining else ['--source-only'])
  import check_planner_owner_final
  check_planner_owner_final.main()
  out=Path(remaining[remaining.index('--output')+1]);scope_path=out/'final-source-scope.json';scope=json.loads(scope_path.read_text());scope.update(inactive_signal_producer=False,producer_source='final-pinned-Carrot',helper_context='explicit active inputs and geometry callbacks');scope_path.write_text(json.dumps(scope,indent=2)+'\n');request=json.loads((out/'input.json').read_text());expected=json.loads((out/'expected.json').read_text());assert len(request['frames'])==len(REQUESTS)
  for index,(frame,signal) in enumerate(zip(request['frames'],REQUESTS,strict=True)):
    frame['signals']=signal
    if args.wire:frame['wire']=WIRE_REQUESTS[index]
  (out/'input.json').write_text(json.dumps(request)+'\n')
  active={'hold':sum(v['signal_owner']['start']['hold'] for v in expected),'prepare':sum(v['signal_owner']['control_active'] for v in expected),'green':sum(v['signal_owner']['green'] for v in expected),'suspended':sum(v['signal_owner']['suspended'] for v in expected)}
  receipt={'source_commit':'8cf5570f6962bd9cb17ec384bbfa12a3a167a5a8','frames':len(expected),'active':active,'source_owner':'final-full-Carrot-and-MPC','geometry':'explicit callback fixture; separate source geometry gate','native_compared':False,'wire_publication':args.wire,'wire_timing':'explicit zero diagnostic durations' if args.wire else None}
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(request).encode());(out/'actual.json').write_bytes(raw);mismatches=difference(expected,json.loads(raw));receipt.update(native_compared=True,mismatch_count=len(mismatches),first_mismatches=mismatches[:100],binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest())
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');assert not mismatches,mismatches[:2]
  (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))
if __name__=='__main__':main()

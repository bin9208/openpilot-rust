import ast
from contextlib import redirect_stdout
import io
from types import SimpleNamespace
from can_source import ROOT
from check_message_state import source as messaging_source
from card_vehicle_source import normalize
from openpilot.cereal import car,log
from openpilot.selfdrive.pandad import can_capnp_to_list,can_list_to_can_capnp
from openpilot.selfdrive.car.card_diagnostics import should_log_card_diagnostics
from openpilot.selfdrive.car.openpilot_toggle import CruiseMainOpenpilotToggle
from openpilot.selfdrive.carrot.xiaoge.xiaoge_vision import parse_xiaoge_vision_payload,apply_xiaoge_vision_result

SERVICES=['pandaStates','carControl','onroadEvents','carrotMan','longitudinalPlan','radarState','modelV2','drivingModelData','customReservedRawData0']


def root(schema,bytes):
  with schema.from_bytes(bytes) as message:
    return message.as_builder()


class Settings:
  def __init__(self): self.values={}; self.param_writes=[]
  def get_bool(self,key): return self.values.get(key,'')=='1'
  def put_bool(self,key,value): self.values[key]='1' if value else '0'
  def put_bool_nonblocking(self,key,value):
    self.param_writes.append([key,list(b'1' if value else b'0')]); self.put_bool(key,value)


def trace(case: dict) -> list[dict]:
  current=case['steps'][0]
  clock=SimpleNamespace(monotonic=lambda: current['now']/1e9,monotonic_ns=lambda:current['now'],thread_time=lambda:0.,sleep=lambda seconds:None)
  messaging,_=messaging_source(); messaging['time']=clock
  sm=messaging['SubMaster'](SERVICES)
  settings=Settings(); calls=[]; tail_calls=[]; publications=[]; warnings=[]; diagnostics={}
  class Driver:
    def __init__(self):
      self.CP=root(car.CarParams,bytes(case['params']))
      self.CC=object() if case['has_controller'] else None
      self.CS=SimpleNamespace(softHoldActive=0,out=car.CarState.new_message())
    def update(self,packets):
      calls.append(dict(call='update',packets=[dict(mono_time=mono,frames=[dict(address=address,data=list(data),bus=bus) for address,data,bus in frames]) for mono,frames in packets],now_ns=current['now']))
      self.CS.out=root(car.CarState,bytes(current['state']))
      return self.CS.out
    def init(self,*args): calls.append(dict(call='init'))
    def apply(self,cc,now,model,radar):
      calls.append(dict(call='apply',now_ns=now,model=model is not None,radar=radar is not None,soft_hold=self.CS.softHoldActive,
                        enabled=cc.enabled,lat_enabled=self.CS.out.latEnabled,activate=self.CS.out.activateCruise))
      actuators=cc.actuators.as_builder(); actuators.accel=current['accel']
      return actuators,[(123,bytes([1,2,3]),4)]
  class Tail:
    _paddle_decel_active=False; v_cruise_kph=20.; v_cruise_cluster_kph=20.; log='fixture'; _soft_hold_active=2
    _activate_cruise=1; _lat_enabled=True; useLaneLineSpeedApply=10.; carrot_cruise_active=True
    def update_v_cruise(self,cs,sm,metric): tail_calls.append(dict(call='tail',metric=metric))
    def initialize_v_cruise(self,previous,experimental): tail_calls.append(dict(call='initialize',speed=previous.vEgo,experimental=experimental))
  def publish(topic,message):
    publications.append(dict(topic=topic,wire=list(message if isinstance(message,bytes) else message.to_bytes())))
  def warning(message): warnings.append(message)
  def record(**values): diagnostics.update(values)
  def decode(bytes):
    with log.Event.from_bytes(bytes) as message: return message
  def update(timeout):
    sm.update_msgs(current['now']/1e9,list(log.Event.read_multiple_bytes(b''.join(bytes(value) for value in current['messages']))))
    if runtime._fixture_settings_at is None or current['now']-runtime._fixture_settings_at>=100_000_000:
      flags=iter((False,True)); runtime.params_thread(SimpleNamespace(is_set=lambda:next(flags)))
      runtime._fixture_settings_at=current['now']
  sm.update=update
  tree=ast.parse((ROOT/'openpilot/selfdrive/car/card.py').read_text())
  node=next(item for item in tree.body if isinstance(item,ast.ClassDef) and item.name=='Car')
  node.body=[item for item in node.body if not isinstance(item,ast.FunctionDef) or item.name in ('state_update','state_publish','controls_update','step','params_thread')]
  scope=dict(car=car,log=log,CarInterfaceBase=object,structs=car,time=clock,REPLAY=case['replay'],EventName=log.OnroadEvent.EventName,
      XIAOGE_LANE_ERROR_LOG_INTERVAL_NS=5_000_000_000,DT_CTRL=.01,cloudlog=SimpleNamespace(warning=warning),
      messaging=SimpleNamespace(drain_sock_raw=lambda sock,wait_for_one: [bytes(value) for value in current['can']],new_message=messaging['new_message'],log_from_bytes=decode),
      can_capnp_to_list=can_capnp_to_list,can_list_to_can_capnp=can_list_to_can_capnp,parse_xiaoge_vision_payload=parse_xiaoge_vision_payload,
      apply_xiaoge_vision_result=apply_xiaoge_vision_result,should_log_card_diagnostics=should_log_card_diagnostics)
  exec(compile(ast.Module(body=[node],type_ignores=[]),str(ROOT/'openpilot/selfdrive/car/card.py'),'exec'),scope)
  import openpilot.selfdrive.pandad.pandad_api_impl as can
  can.time=clock
  import openpilot.selfdrive.car.openpilot_toggle as toggle
  toggle.time=clock
  runtime=scope['Car'].__new__(scope['Car']); runtime.CI=Driver(); runtime.CP=runtime.CI.CP
  runtime.can_sock=object(); runtime.sm=sm; runtime.pm=SimpleNamespace(send=publish); runtime.params=settings; runtime.can_callbacks=(lambda:[],lambda frames:None)
  runtime.CC_prev=car.CarControl.new_message(); runtime.CS_prev=car.CarState.new_message(); runtime.initialized_prev=False
  runtime.cruise_main_toggle=CruiseMainOpenpilotToggle(car.CarState.ButtonEvent.Type.mainCruise)
  runtime.last_actuators_output=car.CarControl.Actuators.new_message(); runtime.can_rcv_cum_timeout_counter=0
  runtime.v_cruise_helper=Tail(); runtime.is_metric=False; runtime.experimental_mode=False; runtime._fixture_settings_at=None
  runtime.rk=SimpleNamespace(remaining=0.); runtime.xiaoge_vision_result=None; runtime.xiaoge_vision_error_log_at_ns=0
  runtime.runtime_diagnostics=SimpleNamespace(record=record)
  for name in ('recv_ns','prev_recv_ns','frames','loop_max_us','process_max_us','slow_loop','slow_process','can_timeouts'):
    setattr(runtime,'card_diag_'+name,0)
  runtime.card_diag_stage_names=('decode','ci_update','sm_update','vision','state_tail','state_total','publish','apply','sendcan','total')
  for name in ('current','sum_us','max_us'): setattr(runtime,'card_diag_stage_'+name,dict.fromkeys(runtime.card_diag_stage_names,0))
  frames=[]
  for current in case['steps']:
    settings.values.update(current['settings']); runtime.rk.remaining=current['remaining']
    buffer=io.StringIO(); error=None
    try:
      with redirect_stdout(buffer): runtime.step()
    except AttributeError as exception:
      if "can_log_mono_time" not in str(exception): raise
      error='replay controls arrived before any CAN timestamp'
    warnings.extend(buffer.getvalue().splitlines())
    frames.append(dict(param_writes=settings.param_writes.copy(),publications=publications.copy(),calls=calls.copy()+tail_calls.copy(),warnings=warnings.copy(),diagnostics=list(diagnostics.items()),
              initialized=runtime.initialized_prev,timeouts=runtime.can_rcv_cum_timeout_counter,
              settings={key:settings.get_bool(key) for key in ('ControlsReady','OpenpilotEnabledToggle','OnroadCycleRequested')},error=error))
    publications.clear();calls.clear();tail_calls.clear();warnings.clear();diagnostics.clear();settings.param_writes.clear()
    if error is not None: break
  return frames


def decoded(frames: list[dict]) -> list[dict]:
  for frame in frames:
    for publication in frame['publications']:
      with log.Event.from_bytes(bytes(publication.pop('wire'))) as message:
        value=message.to_dict()
        if publication['topic']=='sendcan':
          for can in value['sendcan']: can['dat']=list(can['dat'])
        publication['event']=normalize(value)
    frame['diagnostics']=[list(value) for value in frame['diagnostics']]
  return frames

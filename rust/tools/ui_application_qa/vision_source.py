from dataclasses import asdict
import time
from openpilot.cereal import car
from openpilot.selfdrive.ui.vision_status import vision_display_state


def create(scene, ui):
  from openpilot.selfdrive.ui.mici.onroad.vision_renderer import VisionRenderer

  ui.sm.valid={'carState':False,'customReservedRawData0':True}
  ui.sm.alive={'carState':True}
  ui.sm.recv_frame={'carState':0}
  ui.sm['carState']=car.CarState.new_message()
  widget=VisionRenderer()
  widget._owned_car_time=0.0
  return widget


def before(scene, ui, widget, index):
  step=next(step for step in reversed(scene['vision']['steps']) if step['frame']<=index)
  now=10_000_000_000+index*50_000_000 if step['now'] is None else step['now']+(index-step['frame'])*50_000_000
  time.monotonic_ns=lambda:now
  ui.started=step['started']
  ui.started_frame=step['started_frame']
  ui.share_data=step['share']
  ui.sm.updated['carState']=step['car_publish']
  ui.sm.updated['customReservedRawData0']=step['vision_publish']
  if step['car_publish']:
    ui.sm['carState']=car.CarState.new_message(leftBlindspot=step['left'],rightBlindspot=step['right'])
    ui.sm.valid['carState']=step['car_valid']
    ui.sm.recv_frame['carState']=2+index*2
    widget._owned_car_time=index/20
  ui.sm.alive['carState']=index/20-widget._owned_car_time<0.1
  if step['vision_publish']:
    ui.sm['customReservedRawData0']=bytes(step['payload'] or [])
    ui.sm.valid['customReservedRawData0']=step['vision_valid']


def snapshot(ui, widget):
  state=asdict(vision_display_state(widget._packet,time.monotonic_ns()))
  latency=state['latency_ms']
  if latency is not None:
    state['latency_ms']={'kind':'integer' if isinstance(latency,int) else 'float','value':str(latency) if isinstance(latency,int) else latency}
  return {'state':state,'car_fresh':bool(ui.sm.valid['carState'] and ui.sm.alive['carState'] and ui.sm.recv_frame['carState']>=ui.started_frame)}

"""Final passive inset using real source renderer/VisionIPC, isolated UI state."""
import os
from pathlib import Path
import sys
import time
from msgq.visionipc import VisionIpcClient, VisionStreamType
from openpilot.cereal import log
from runtime_peer import SERVICES


def create(scene,ui):
  sys.modules['openpilot.system.hardware'].TICI=False
  from openpilot.selfdrive.ui.onroad.driver_preview import DriverPreview
  from openpilot.system.ui.widgets import Widget
  ui.sm.recv_frame=dict.fromkeys(SERVICES,0)
  ui.sm.recv_time=dict.fromkeys(SERVICES,0.)
  ui.sm.logMonoTime=dict.fromkeys(SERVICES,0)
  for attribute in ('alive','valid','updated','seen'):
    setattr(ui.sm,attribute,dict.fromkeys(SERVICES,False))
  for name in SERVICES:
    event=log.Event.new_message()
    event.init(name,0) if name in ('pandaStates','onroadEvents','customReservedRawData0') else event.init(name)
    value=getattr(event,name)
    ui.sm[name]=list(value) if name in ('pandaStates','onroadEvents') else bytes(value) if name=='customReservedRawData0' else value
  ui.started_frame=0;ui.started_time=0.;ui.always_on_dm=False
  preview=DriverPreview(compact=not scene['config']['big'])
  preview._name='rustvision';preview.client=VisionIpcClient('rustvision',VisionStreamType.VISION_STREAM_DRIVER,conflate=True)
  class Wrapper(Widget):
    def _render(self,rect):
      preview.draw_onroad(rect,ui.sm['selfdriveState'].alertSize.raw!=0)
    def set_cluster_hud_connected(self,*args):
      pass
  class Camera:
    def before(self,index):
      directory=Path(os.environ['UI_CAMERA_SYNC']);(directory/f'{index}.ready').write_text('ready')
      start=time.perf_counter()
      while not (directory/f'{index}.allow').exists():
        assert time.perf_counter()-start<15
        time.sleep(.001)
    def snapshot(self):
      return {'driverView':ui.params.get_bool('IsDriverViewEnabled')}
  return Wrapper(),Camera()

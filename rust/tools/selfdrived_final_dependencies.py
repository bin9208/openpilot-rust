"""Load only pinned original definitions; controlled IO is supplied by fixture."""
import ast
import bisect
from dataclasses import dataclass
import math
import subprocess
import hashlib
import numpy as np
from openpilot.common.transformations.orientation import rot_from_euler
PIN='e8e152dbcfd9574a1791ed12026d1880e87689b3'
HASHES={}
def text(path):
  return subprocess.check_output(['git','show',f'{PIN}:{path}'],text=True)
def definitions(path,names,namespace):
  source=text(path)
  HASHES[path]=hashlib.sha256(source.encode()).hexdigest()
  tree=ast.parse(source)
  nodes=[n for n in tree.body if isinstance(n,(ast.ClassDef,ast.FunctionDef)) and n.name in names or isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id in names for t in n.targets)]
  future=ast.ImportFrom(module='__future__',names=[ast.alias(name='annotations')],level=0)
  exec(compile(ast.fix_missing_locations(ast.Module(body=[future,*nodes],type_ignores=[])),path+':'+PIN,'exec'),namespace)
def load():
  ns={'math':math,'bisect':bisect,'json':__import__('json'),'np':np,'rot_from_euler':rot_from_euler,'dataclass':dataclass,'Any':object}
  definitions('openpilot/common/impact_dashcam.py',{'NOTICE_KEY','FEEDBACK_KEY','REBOOT_KEY','COUNTDOWN_SECONDS','UI_TIMEOUT','read_object','ImpactDashcam'},ns)
  definitions('openpilot/selfdrive/selfdrived/impact_detector.py',{'IMPACT_ACCEL','REARM_ACCEL','MAX_SAMPLE_AGE','MAX_SAMPLE_GAP','ImpactDetector'},ns)
  definitions('openpilot/selfdrive/carrot/radar_motion/primary.py',{'RadarPointSnapshot','_finite','_normalized_source','snapshot_live_radar_points'},ns)
  definitions('openpilot/selfdrive/carrot/radar_motion/stationary_lane.py',{'_line','_at','_Lane','confident_ego_lane'},ns)
  ns['CORNER_RADAR_MEASUREMENT_DELAY_S']=.05
  definitions('openpilot/selfdrive/controls/lib/radar_only_warning.py',{'RadarOnlyWarning'},ns)
  definitions('openpilot/selfdrive/carrot/radar_motion/timing.py',{'VOLKSWAGEN_MEB_FLAG','front_radar_distance_delay_s'},ns)
  return ns
class Memory(dict):
  def get_bool(self,key):return bool(self.get(key,False))
  def put(self,key,value):self[key]=value
  def put_bool(self,key,value):self[key]=value
  def remove(self,key):self.pop(key,None)

import ast
from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace
import numpy as np
import pyray as rl
from openpilot.cereal import log


class Widget:
  def __init__(self):
    self._rect = rl.Rectangle(0, 0, 0, 0)

  def set_visible(self, callback):
    self.visible = callback

  @property
  def is_visible(self):
    return self.visible()


def render(steps):
  ui = SimpleNamespace(started_frame=0, sm=SimpleNamespace())
  class Messages(dict):
    recv_frame = {'driverStateV2': 1}
  ui.sm = Messages(selfdriveState=SimpleNamespace(alertSize=0))
  namespace = {'__name__': __name__, 'np': np, 'rl': rl, 'log': log, 'dataclass': dataclass, 'Widget': Widget,
               'UI_BORDER_SIZE': 30, 'ui_state': ui, 'gui_app': SimpleNamespace(texture=lambda *args: SimpleNamespace(width=144,height=144))}
  tree = ast.parse((Path(__file__).resolve().parents[3]/'openpilot/selfdrive/ui/onroad/driver_state.py').read_text())
  nodes = [node for node in tree.body if isinstance(node,(ast.Assign,ast.ClassDef))]
  exec(compile(ast.Module(body=nodes,type_ignores=[]),'driver_state.py','exec'),namespace)
  cls = namespace['DriverStateRenderer']
  renderer = cls()
  outputs = []
  def points(values):
    return [{'x': point.x, 'y': point.y} for point in values]
  for step in steps:
    if step.get('reset'):
      renderer = cls()
    data = SimpleNamespace(faceOrientation=step['orientation'])
    ui.sm['driverStateV2'] = SimpleNamespace(leftDriverData=data,rightDriverData=data)
    ui.sm['driverMonitoringState'] = SimpleNamespace(activePolicy=log.DriverMonitoringState.MonitoringPolicy.vision if step['active'] else 0,isRHD=step['rhd'])
    renderer._rect = rl.Rectangle(*(step['rect'][key] for key in ['x','y','width','height']))
    renderer._update_state()
    outputs.append({'fade':float(renderer.dm_fade_state),'pose':renderer.driver_pose_vals.tolist(),'difference':renderer.driver_pose_diff.tolist(),
                    'sins':renderer.driver_pose_sins.tolist(),'coss':renderer.driver_pose_coss.tolist(),'face':renderer.face_kpts_draw.tolist(),
                    'transformed':renderer.face_keypoints_transformed.tolist(),'lines':points(renderer.face_lines),
                    'center':[renderer.position_x,renderer.position_y],
                    'horizontal':{'points':points(renderer.h_arc_lines),'thickness':float(renderer.h_arc_data.thickness)} if renderer.h_arc_data else None,
                    'vertical':{'points':points(renderer.v_arc_lines),'thickness':float(renderer.v_arc_data.thickness)} if renderer.v_arc_data else None})
  return outputs

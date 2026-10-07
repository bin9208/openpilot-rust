import ast
from collections import OrderedDict
from functools import wraps
import math
from pathlib import Path

import numpy as np


def cache():
  tree=ast.parse((Path(__file__).resolve().parents[3]/'openpilot/selfdrive/ui/mici/onroad/torque_bar.py').read_text())
  nodes=[node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name in ['quantized_lru_cache','arc_bar_pts']]
  namespace={'math':math,'np':np,'wraps':wraps,'OrderedDict':OrderedDict,'DEBUG':False}
  exec(compile(ast.Module(body=nodes,type_ignores=[]),'torque_bar.py','exec'),namespace)
  return namespace['arc_bar_pts']


def render(steps):
  arc=cache()
  outputs=[]
  for step in steps:
    if step.get('reset'):
      arc=cache()
    a=step['arc']
    outputs.append([{'x':float(x),'y':float(y)} for x,y in arc(a['cx'],a['cy'],a['radius'],a['thickness'],a['start'],a['end'])])
  return outputs

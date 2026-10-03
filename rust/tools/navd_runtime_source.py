from __future__ import annotations

import argparse
import ast
import hashlib
import json
import math
import os
from pathlib import Path
import threading
import time

import numpy as np
import requests

from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--host', required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  binding, swaglog = load(args.binding.resolve(), 'ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX'], args.output / 'logs')
  from openpilot.cereal import log, messaging
  from openpilot.common.utils import MovingAverage
  from openpilot.selfdrive.navd import helpers
  import msgq.ipc_pyx
  origins = {name: {'path': path, 'sha256': hashlib.sha256(Path(path).read_bytes()).hexdigest()}
             for name, path in {'params': binding.__file__, 'ipc': msgq.ipc_pyx.__file__}.items()}
  (args.output / 'source-bindings.json').write_text(json.dumps(origins, indent=2) + '\n')
  realtime = ast.parse((ROOT / 'openpilot/common/realtime.py').read_text())
  ratekeeper = next(node for node in realtime.body if isinstance(node, ast.ClassDef) and node.name == 'Ratekeeper')
  namespace = {'time': time, 'MovingAverage': MovingAverage, 'getproctitle': lambda: 'openpilot.selfdrive.navd.navd'}
  exec(compile(ast.Module(body=[ratekeeper], type_ignores=[]), 'original-ratekeeper', 'exec'), namespace)
  namespace.update({'math': math, 'np': np, 'os': os, 'json': json, 'log': log, 'requests': requests,
                    'Params': lambda: binding.Params(os.environ['PARAMS_ROOT']), 'threading': threading,
                    'messaging': messaging, 'cloudlog': swaglog.cloudlog})
  namespace.update({name: getattr(helpers, name) for name in ('Coordinate', 'coordinate_from_param', 'distance_along_geometry',
                   'maxspeed_to_ms', 'minimum_distance', 'parse_banner_instructions')})
  tree = ast.parse((ROOT / 'openpilot/selfdrive/navd/navd.py').read_text())
  constants = {'REROUTE_DISTANCE', 'MANEUVER_TRANSITION_THRESHOLD', 'REROUTE_COUNTER_MIN', 'NAV_ROUTE_MAX_POINTS'}
  nodes = [node for node in tree.body if (isinstance(node, (ast.ClassDef, ast.FunctionDef)) and
           node.name in ('RouteEngine', 'limit_route_points', 'main')) or (isinstance(node, ast.Assign) and
           any(isinstance(target, ast.Name) and target.id in constants for target in node.targets))]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'original-navd-runtime', 'exec'), namespace)
  original = namespace['RouteEngine']

  def owned_endpoint(sm, pm):
    engine = original(sm, pm)
    engine.mapbox_host = args.host
    return engine

  namespace['RouteEngine'] = owned_endpoint
  try:
    namespace['main']()
  except KeyboardInterrupt:
    swaglog.cloudlog.warning('child openpilot.selfdrive.navd.navd got SIGINT')


if __name__ == '__main__':
  main()

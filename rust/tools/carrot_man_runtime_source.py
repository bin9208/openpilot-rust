import argparse
import ast
import asyncio
import base64
import errno
import hashlib
import ipaddress
import json
import math
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import threading
import time
import traceback
from datetime import datetime
from types import SimpleNamespace as NS
import msgq

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(Path(__file__).parent))


class LiveClock:
  def __getitem__(self, index):
    return time.monotonic()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binding", required=True)
  args = parser.parse_args()
  fixture = json.load(sys.stdin)
  root = Path(fixture["root"])
  from carrot_man_serv_compare import source_owner
  serv, params, memory = source_owner(root / "params", args.binding, LiveClock())
  sys.modules["openpilot.common.params"] = NS(Params=lambda path=None: memory if path else params)
  from openpilot.cereal import log, messaging
  from openpilot.common.constants import CV
  from openpilot.common.utils import MovingAverage
  from openpilot.selfdrive.navd.helpers import Coordinate
  from openpilot.selfdrive.carrot.curve_speed import VisionCurveSpeed, curve_speed
  from openpilot.selfdrive.carrot.carrot_navi_control import CarrotNaviControl
  from openpilot.selfdrive.carrot.navigation_ingress import (
    NavigationIngress, NavigationPeerState, bind_tmap_legacy_frame,
    build_legacy_http_session_id, handle_navigation_udp_datagram, serve_navigation_tcp,
  )
  from openpilot.selfdrive.carrot.navigation_sources import NavigationSource
  from openpilot.selfdrive.carrot.server.services import web_settings
  from openpilot.selfdrive.carrot.web_upload import (
    carrot_logs_web_target, create_web_upload_session_sync, post_tmux_web,
    tmux_web_target, web_upload_settings,
  )
  from aiohttp import web
  import numpy as np
  import psutil
  import requests
  import zmq
  try:
    from shapely.geometry import LineString
    shapely_available = True
  except ImportError:
    LineString = None
    shapely_available = False

  scope = dict(globals(), **locals())
  scope.update(CarrotServ=lambda: serv, Params=lambda path=None: memory if path else params,
    ParamKeyType=NS(BYTES=6, JSON=5), read_web_settings=web_settings.read_web_settings,
    PC=True, TICI=False, get_gps_location_service=lambda p: "gpsLocationExternal" if p.get_bool("UbloxAvailable") else "gpsLocation",
    getproctitle=lambda: "carrot-man-source", SHAPELY_AVAILABLE=shapely_available)
  realtime = ast.parse((ROOT / "openpilot/common/realtime.py").read_text())
  ratekeeper = next(node for node in realtime.body if isinstance(node, ast.ClassDef) and node.name == "Ratekeeper")
  prefix = ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0)
  exec(compile(ast.fix_missing_locations(ast.Module(body=[prefix, ratekeeper], type_ignores=[])), "realtime.py", "exec"), scope)
  ports = dict(zip((7706, 7712, 7713, 7709, 12345, 7710, 7705), fixture["ports"]))
  data = root / "data"
  (data / "media").mkdir(parents=True, exist_ok=True)
  (data / "params/d").mkdir(parents=True, exist_ok=True)
  backup = root / "apilot-source.py"
  backup.write_text((ROOT / "openpilot/selfdrive/apilot.py").read_text().replace("/data/params/d", str(data / "params/d")).replace("/data/backup_params.json", str(data / "backup_params.json")))
  backup.chmod(0o700)
  paths = {"/data/media/tmux.log": str(data / "media/tmux.log"), "/data/toggle_values.json": str(data / "toggle_values.json"),
           "/data/openpilot/openpilot/selfdrive/apilot.py": str(backup)}
  class Boundary(ast.NodeTransformer):
    def visit_Constant(self, node):
      if type(node.value) is int and node.value in ports:
        return ast.copy_location(ast.Constant(ports[node.value]), node)
      if type(node.value) is str:
        value = node.value
        for original, owned in paths.items():
          value = value.replace(original, owned)
        value = value.replace("tcp://*:7710", f"tcp://127.0.0.1:{ports[7710]}")
        if value == "0.0.0.0":
          value = "127.0.0.1"
        return ast.copy_location(ast.Constant(value), node)
      return node
  tree = ast.parse((ROOT / "openpilot/selfdrive/carrot/carrot_man.py").read_text())
  nodes = [node for node in tree.body if isinstance(node, (ast.Assign, ast.FunctionDef, ast.ClassDef)) and getattr(node, "name", "") != "main"]
  executable = Boundary().visit(ast.Module(body=[prefix, *nodes], type_ignores=[]))
  exec(compile(ast.fix_missing_locations(executable), "carrot_man.py", "exec"), scope)
  web_settings.CARROT_WEB_SETTINGS_PATH = str(root / "web_settings.json")
  scope["CarrotMan"].get_broadcast_address = lambda self: "127.0.0.1"
  scope["CarrotMan"].get_local_ip = lambda self, destination_ip="8.8.8.8": "127.0.0.1"
  owner = scope["CarrotMan"]()
  for target in (owner.kisa_app_thread, owner.carrot_navi_thread, owner.carrot_navi_http_thread, owner.carrot_man_thread):
    threading.Thread(target=target, daemon=True).start()
  (root / "source-configuration.json").write_text(json.dumps(dict(
    adaptations="original AST with owned path/port and loopback address boundaries; real original Params, msgq, HTTP and sockets",
    project_python=True, versions=dict(aiohttp=__import__("aiohttp").__version__, psutil=psutil.__version__, numpy=np.__version__),
    command=[sys.executable, __file__, "--binding", args.binding]), indent=2) + "\n")
  while True:
    time.sleep(1)


if __name__ == "__main__":
  main()

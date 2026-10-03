#!/usr/bin/env python3
import argparse
import ast
import base64
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import time
from types import SimpleNamespace
import numpy as np
from openpilot.cereal import log
from athena_fixture import daemon, private_environment, published, wait_for, websocket_server
from athena_reference import ROOT, ParamsStore
from check_athena_image import snapshot_source


def camera(frame):
  packet = log.Event.new_message()
  state = packet.init('wideRoadCameraState')
  state.frameId = frame
  return packet.to_bytes()


def original_gate(offroad, taking, front, pc, running):
  params = ParamsStore()
  params.values.update(IsOffroad=offroad, IsTakingSnapshot=taking, RecordFront=front)
  events = []

  def check(_):
    if not running:
      raise subprocess.CalledProcessError(1, ['pgrep', 'camerad'])

  def alert(_, show):
    events.append(['alert', show])

  scope = {'Params': params, 'time': SimpleNamespace(sleep=lambda seconds: events.append(['sleep', seconds])), 'subprocess': SimpleNamespace(check_call=check, CalledProcessError=subprocess.CalledProcessError), 'PC': pc, 'set_offroad_alert': alert, 'managed_processes': {'camerad': SimpleNamespace(start=lambda: events.append(['start']), stop=lambda: events.append(['stop']))}, 'get_snapshots': lambda *args: ('rear', 'front')}
  tree = ast.parse((ROOT / 'openpilot/system/camerad/snapshot.py').read_text())
  body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'snapshot']
  exec(compile(ast.Module(body=body, type_ignores=[]), 'snapshot.py', 'exec'), scope)
  return {'return': scope['snapshot'](), 'events': events, 'params': params.values, 'writes': params.writes}


def setup_alert(env):
  destination = env.root / 'openpilot/selfdrive/selfdrived'
  destination.mkdir(parents=True)
  (destination / 'alerts_offroad.json').write_bytes((ROOT / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_bytes())


def expected_jpeg():
  source = snapshot_source()
  image = source['extract_image'](SimpleNamespace(width=8, height=4, stride=16, uv_offset=64, data=np.arange(320, dtype=np.uint16).astype(np.uint8).tolist()))
  output = io.BytesIO()
  source['jpeg_write'](output, image)
  return output.getvalue()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('ipc_peer', type=Path)
  parser.add_argument('vision_peer', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False, 'source_gates': [original_gate(*case) for case in [(False, False, False, True, False), (True, True, True, True, False), (True, False, True, True, False), (True, False, False, True, False), (True, False, True, False, False), (True, False, True, True, True)]]}
  expected = expected_jpeg()
  with websocket_server() as (port, connected, _), private_environment(port) as env:
    setup_alert(env)
    vision_log = (args.output / 'vision.log').open('wb')
    vision = subprocess.Popen([args.vision_peer], env=dict(env.env, ATHENA_VISION_FIXTURE_AUTO='1'), stdout=vision_log, stderr=subprocess.STDOUT)
    try:
      wait_for(lambda: 'READY' in (args.output / 'vision.log').read_text())
      with published(args.ipc_peer, 'wideRoadCameraState', env.env, camera(79)) as packets, daemon(args.binary, args.output, env.env) as (process, pid):
        peer = connected.get(timeout=10)
        assert peer.rpc('takeSnapshot')['result'] == {'jpegBack': None, 'jpegFront': None}
        (env.params / 'IsOffroad').write_text('1')
        (env.params / 'RecordFront').write_text('1')
        peer.socket.send(json.dumps({'jsonrpc': '2.0', 'id': 100, 'method': 'takeSnapshot'}))
        wait_for(lambda: (env.params / 'IsTakingSnapshot').read_text() == '1' if (env.params / 'IsTakingSnapshot').exists() else False)
        wait_for(lambda: (env.params / 'Offroad_IsTakingSnapshot').exists())
        peer.socket.send(json.dumps({'jsonrpc': '2.0', 'id': 101, 'method': 'takeSnapshot'}))
        concurrent = peer.replies.get(timeout=3)
        assert concurrent['id'] == 101 and concurrent['result'] == {'jpegBack': None, 'jpegFront': None}
        time.sleep(2.2)
        assert peer.replies.empty(), peer.replies.get_nowait()
        assert (env.params / 'IsTakingSnapshot').read_text() == '1'
        packets[0] = camera(80)
        response = peer.replies.get(timeout=5)
        assert response['id'] == 100 and 'result' in response, response
        for field in ['jpegBack', 'jpegFront']:
          actual = base64.b64decode(response['result'][field])
          assert actual == expected
          (args.output / f'{field}.jpg').write_bytes(actual)
        assert (env.params / 'IsTakingSnapshot').read_text() == '0'
        assert not (env.params / 'Offroad_IsTakingSnapshot').exists()
        result['front_and_rear_exact'] = True
        (env.params / 'RecordFront').write_text('0')
        response = peer.rpc('takeSnapshot', timeout=5)['result']
        assert base64.b64decode(response['jpegBack']) == expected and response['jpegFront'] is None
        result['front_disabled'] = True
        packets[0] = camera(79)
        peer.socket.send(json.dumps({'jsonrpc': '2.0', 'id': 102, 'method': 'takeSnapshot'}))
        wait_for(lambda: (env.params / 'IsTakingSnapshot').read_text() == '1')
        time.sleep(2.2)
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        assert (env.params / 'IsTakingSnapshot').read_text() == '0'
        assert not (env.params / 'Offroad_IsTakingSnapshot').exists()
        result.update(frame79_waited=True, frame80_completed=True, concurrent_snapshot_gated=True, termination_cleaned_params=True, returncode=process.returncode)
      result['pass'] = True
    finally:
      if vision.poll() is None:
        vision.send_signal(signal.SIGTERM)
      vision.wait(timeout=5)
      vision_log.close()
      result['vision_returncode'] = vision.returncode
      (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: native RPC snapshots through real VisionIPC,cereal frame80 gate,exact source JPEGs,front permission,concurrency gate and termination cleanup')


if __name__ == '__main__':
  main()

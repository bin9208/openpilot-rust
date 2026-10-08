from __future__ import annotations

import ast
import asyncio
from collections import deque
import json
import ipaddress
from pathlib import Path
import socket
import sys
import types
import urllib.error
import urllib.request
from urllib.parse import urlsplit

from aiohttp import web
from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def sequence(values):
  queue = deque(values)
  last = values[-1]
  def take():
    nonlocal last
    if queue:
      last = queue.popleft()
    return last
  return take


def original(config):
  path = ROOT / 'openpilot/selfdrive/carrot/server/services/heartbeat.py'
  tree = ast.parse(path.read_text(), filename=str(path))
  tree.body = [node for node in tree.body if not isinstance(node, ast.ImportFrom) or node.level == 0]
  module = types.ModuleType('original_heartbeat')
  module.HAS_PARAMS = config['has_params']
  module.Params = None
  if config['has_params']:
    root = Path(config['params_root'])
    extension, _ = load(config['binding'], f'ipc://{root}/heartbeat-log', root / 'logs')
    module.Params = lambda: extension.Params(config['params_root'])
  exec(compile(tree, str(path), 'exec'), module.__dict__)
  module.original_get_local_ip = module.get_local_ip
  module.get_local_ip = sequence(config['ips'])
  module.time = types.SimpleNamespace(time=sequence(config['times']))
  real_open = urllib.request.urlopen
  observations = []
  def owned_open(request, **kwargs):
    observations.append(dict(source_url=request.full_url, method=request.get_method(), headers=request.header_items(), payload=request.data.hex(), timeout=kwargs['timeout'], unverified=kwargs['context'].verify_mode == 0))
    request = urllib.request.Request(config['endpoint'], data=request.data, headers=request.headers, method=request.get_method())
    return real_open(request, **kwargs)
  module.urllib = types.SimpleNamespace(error=urllib.error, request=types.SimpleNamespace(Request=urllib.request.Request, urlopen=owned_open))
  return module, observations


def owned_ip(module, command):
  host, port = command['route_peer'].rsplit(':', 1)
  assert ipaddress.ip_address(host).is_loopback
  class Route:
    def __enter__(self):
      self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
      return self
    def __exit__(self, *args):
      self.socket.close()
    def connect(self, address):
      assert address == ('8.8.8.8', 80)
      if not command['route_ok']:
        raise OSError('owned route unavailable')
      self.socket.connect((host, int(port)))
    def getsockname(self):
      return self.socket.getsockname()
  def hostname():
    if command['hostname'] is None:
      raise OSError('owned hostname unavailable')
    assert command['hostname'] == 'localhost'
    return command['hostname']
  module.socket = types.SimpleNamespace(AF_INET=socket.AF_INET, SOCK_DGRAM=socket.SOCK_DGRAM, socket=lambda *args: Route(), gethostname=hostname, gethostbyname=socket.gethostbyname)
  try:
    return module.original_get_local_ip()
  finally:
    module.socket = socket


def initial():
  path = ROOT / 'openpilot/selfdrive/carrot/server/app.py'
  tree = ast.parse(path.read_text())
  startup = next(node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_startup')
  value = next(node.value for node in startup.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Subscript) and isinstance(target.slice, ast.Constant) and target.slice.value == 'hb_last' for target in node.targets))
  return ast.literal_eval(value)


def status_handler():
  path = ROOT / 'openpilot/selfdrive/carrot/server/features/system.py'
  tree = ast.parse(path.read_text(), filename=str(path))
  tree.body = [node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'api_heartbeat_status']
  namespace = dict(web=web)
  exec(compile(tree, str(path), 'exec'), namespace)
  return namespace['api_heartbeat_status']


async def main():
  config = json.loads(sys.stdin.readline())
  assert ipaddress.ip_address(urlsplit(config['endpoint']).hostname).is_loopback
  module, observations = original(config)
  app = web.Application()
  app['hb_last'] = initial()
  app.router.add_get('/api/heartbeat_status', status_handler())
  runner = web.AppRunner(app)
  await runner.setup()
  site = web.TCPSite(runner, '127.0.0.1', 0)
  await site.start()
  task = None
  print(json.dumps(dict(port=site._server.sockets[0].getsockname()[1])), flush=True)
  while line := await asyncio.to_thread(sys.stdin.readline):
    command = json.loads(line)
    match command['operation']:
      case 'register': result = await asyncio.to_thread(module.register_my_ip_sync, module.Params())
      case 'start':
        task = asyncio.create_task(module.heartbeat_loop(app))
        await asyncio.sleep(0)
        result = None
      case 'stop':
        task.cancel()
        try:
          await task
          result = 'returned'
        except asyncio.CancelledError:
          result = 'cancelled'
        task = None
      case 'status': result = app.get('hb_last')
      case 'observations': result = observations
      case 'ip': result = owned_ip(module, command)
      case 'verified_tls':
        try:
          with urllib.request.urlopen(config['endpoint'], timeout=3.5):
            result = True
        except urllib.error.URLError:
          result = False
      case operation: raise ValueError(operation)
    print(json.dumps(dict(result=result), ensure_ascii=True), flush=True)
  if task:
    task.cancel()
    try:
      await task
    except asyncio.CancelledError:
      pass
  await runner.cleanup()


if __name__ == '__main__':
  asyncio.run(main())

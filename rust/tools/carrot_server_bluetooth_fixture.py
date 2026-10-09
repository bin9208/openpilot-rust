import asyncio
from contextlib import asynccontextmanager
import json
import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
from aiohttp import web
from jeepney.io.asyncio import open_dbus_router


def write(path, value):
  path.parent.mkdir(parents=True, exist_ok=True)
  path.write_text(json.dumps(value, ensure_ascii=False))


def source_module(root):
  package = ModuleType('openpilot.selfdrive.carrot.server.features')
  package.__path__ = [str(Path.cwd() / 'openpilot/selfdrive/carrot/server/features')]
  sys.modules[package.__name__] = package
  from openpilot.selfdrive.carrot.server.features import bluetooth as source
  from openpilot.selfdrive.carrot.bluetooth import model
  source.CONFIG_PATH = model.CONFIG_PATH = root / 'config.json'
  model.config.__defaults__ = (root / 'config.json',)
  source.RUNTIME = model.RUNTIME = root / 'runtime'
  source.time = SimpleNamespace(monotonic=lambda: 100.)
  return source


def setup(root):
  write(root / 'runtime/status.json', {'time': 100, 'stationary': True})
  (root / 'bin').mkdir()
  command = root / 'bin/sudo'
  command.write_text('''#!/usr/bin/python3
import json, pathlib, sys, time
root = pathlib.Path(__file__).parent.parent
args = sys.argv[1:]
with (root / 'commands.jsonl').open('a') as output: output.write(json.dumps(args) + '\\n')
with (root / 'command-pids.jsonl').open('a') as output: output.write(json.dumps({'pid': __import__('os').getpid(), 'args': args}) + '\\n')
control = json.loads((root / 'command-control.json').read_text()) if (root / 'command-control.json').exists() else {}
if control.get('hold') == args[1]: time.sleep(100)
if control.get('fail') == args[1]:
  sys.stderr.buffer.write(bytes.fromhex(control.get('error', '6661696c6564')))
  sys.exit(2)
marker = root / 'radio-enabled'
if args[1] == 'test': sys.exit(0 if marker.exists() else 1)
if args[1] == 'touch': marker.touch()
if args[1] == 'rm': marker.unlink(missing_ok=True)
''')
  command.chmod(0o755)
  (root / 'runtime/commands.jsonl').write_text('daemon-only\n')
  return command


def files(root):
  result = {}
  for relative in ('config.json', 'runtime/cancelled.json', 'runtime/learn.json', 'runtime/commands.jsonl'):
    path = root / relative
    if path.exists():
      result[relative] = {'bytes': path.read_bytes().hex(), 'mode': oct(path.stat().st_mode & 0o777)}
  result['temporary_files'] = [str(path.relative_to(root)) for path in root.rglob('*.tmp')]
  return result


@asynccontextmanager
async def server(native, binary, root, bus, command, output, composed=False):
  if native:
    process = await asyncio.create_subprocess_exec(str(binary), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    settings = {'bus': bus, 'config': str(root / 'config.json'), 'runtime': str(root / 'runtime'), 'command': str(command), 'timestamp': 100., 'composed': composed}
    process.stdin.write((json.dumps(settings) + '\n').encode())
    await process.stdin.drain()
    first = await asyncio.wait_for(process.stdout.readline(), 10)
    port = json.loads(first)['port']
    try:
      yield port
    finally:
      process.stdin.write(b'stop\n')
      await process.stdin.drain()
      stdout, stderr = await asyncio.wait_for(process.communicate(), 15)
      (output / 'native-process.json').write_text(json.dumps({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()}, indent=2))
      assert process.returncode == 0, stderr
  else:
    source = source_module(root)
    from openpilot.selfdrive.carrot.bluetooth import bluez
    bluez.open_dbus_router = lambda _bus: open_dbus_router(bus)
    app = web.Application()
    source.register(app)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    try:
      yield site._server.sockets[0].getsockname()[1]
    finally:
      await runner.cleanup()

from contextlib import asynccontextmanager
import json
import select
from pathlib import Path
import subprocess
import tempfile

import anyio
from jeepney import DBusAddress, HeaderFields, MatchRule, new_error, new_method_call, new_method_return
from jeepney.io.asyncio import open_dbus_router
from jeepney.wrappers import unwrap_msg

AGENT = '/org/carrot/BluetoothAgent'
DEVICE = 'org.bluez.Device1'
ADAPTER = 'org.bluez.Adapter1'
PATH = '/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF'
MAC = 'AA:BB:CC:DD:EE:FF'


class Server:
  def __init__(self, router, tasks):
    self.router = router
    self.tasks = tasks
    self.calls = []
    self.agent_replies = []
    self.client = None
    self.mode = 'plain'
    self.pending = None
    self.pair_received = anyio.Event()
    self.discovery_stopped = anyio.Event()
    self.objects = {
      '/org/bluez/hci1': {ADAPTER: {'Address': ('s', '11:22:33:44:55:66'), 'Powered': ('b', True), 'Discovering': ('b', False)}},
      '/org/bluez/hci0': {ADAPTER: {}},
      PATH: {DEVICE: {'Address': ('s', MAC.lower()), 'Name': ('s', '리모컨'), 'Alias': ('s', 'ignored alias'), 'RSSI': ('n', -58),
                     'UUIDs': ('as', ['00001124-0000-1000-8000-00805f9b34fb'])}, 'org.bluez.Battery1': {'Percentage': ('y', 98)}},
      '/org/bluez/hci0/dev_00_00_00_00_00_01': {DEVICE: {'Address': ('s', '00:00:00:00:00:01'), 'Alias': ('s', 'fallback')}},
      '/unknown': {'org.fixture.Other': {}},
    }

  async def agent(self, method, signature=None, body=(), interface='org.bluez.Agent1', router=None, reply_seconds=3):
    router = router or self.router
    request = new_method_call(DBusAddress(AGENT, bus_name=self.client, interface=interface), method, signature, body)
    with anyio.fail_after(reply_seconds):
      reply = await router.send_and_get_reply(request)
    row = {'method': method, 'error': reply.header.fields.get(HeaderFields.error_name),
           'body': list(reply.body), 'signature': reply.header.fields.get(HeaderFields.signature, '')}
    self.agent_replies.append(row)
    return row

  async def dispatch(self, message):
    path = message.header.fields.get(HeaderFields.path)
    interface = message.header.fields.get(HeaderFields.interface)
    method = message.header.fields.get(HeaderFields.member)
    self.calls.append({'path': path, 'interface': interface, 'method': method, 'body': message.body})
    if method == 'GetManagedObjects':
      response = new_method_return(message, 'a{oa{sa{sv}}}', (self.objects,))
    elif method == 'RegisterAgent':
      self.client = message.header.fields[HeaderFields.sender]
      response = new_method_return(message)
    elif method in ('StartDiscovery', 'StopDiscovery'):
      self.objects[path][ADAPTER]['Discovering'] = ('b', method == 'StartDiscovery')
      if method == 'StopDiscovery':
        self.discovery_stopped.set()
      response = new_method_return(message)
    elif method == 'Pair':
      if self.mode == 'failure':
        response = new_error(message, 'org.bluez.Error.AuthenticationRejected', 's', ("owned 'failure' 한글",))
      elif self.mode == 'hold':
        self.pending = message
        self.pair_received.set()
        return
      elif self.mode == 'confirm':
        answer = await self.agent('RequestConfirmation', 'ou', (path, 123))
        response = (new_error(message, 'org.bluez.Error.AuthenticationRejected', 's', ('rejected',)) if answer['error']
                    else new_method_return(message))
      else:
        response = new_method_return(message)
    elif method == 'CancelPairing':
      self.pending = None
      response = new_method_return(message)
    elif method == 'Set':
      requested_interface, key, value = message.body
      self.objects[path][requested_interface][key] = value
      response = new_method_return(message)
    elif method == 'Connect' and self.mode == 'connect-failure':
      response = new_error(message, 'org.bluez.Error.Failed', 's', ('connect failed',))
    elif method in ('Connect', 'Disconnect', 'RemoveDevice'):
      response = new_method_return(message)
    else:
      response = new_error(message, 'org.bluez.Error.NotSupported')
    await self.router.send(response)

  async def receive(self, queue):
    while True:
      self.tasks.start_soon(self.dispatch, await queue.get())


@asynccontextmanager
async def owned_bus():
  with tempfile.TemporaryDirectory(prefix='bluez155-') as temporary:
    bus = await anyio.to_thread.run_sync(lambda: subprocess.Popen(
      ['dbus-daemon', '--session', '--nofork', '--nopidfile', '--print-address=1', f'--address=unix:path={temporary}/bus'],
      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True))
    try:
      assert bus.stdout is not None
      address = (await anyio.to_thread.run_sync(bus.stdout.readline)).strip()
      assert address.startswith('unix:path='), address
      async with open_dbus_router(address) as router:
        request = new_method_call(DBusAddress('/org/freedesktop/DBus', bus_name='org.freedesktop.DBus',
                                  interface='org.freedesktop.DBus'), 'RequestName', 'su', ('org.bluez', 0))
        assert unwrap_msg(await router.send_and_get_reply(request)) == (1,)
        async with anyio.create_task_group() as tasks:
          server = Server(router, tasks)
          with router.filter(MatchRule(type='method_call'), bufsize=64) as queue:
            tasks.start_soon(server.receive, queue)
            try:
              yield address, server
            finally:
              tasks.cancel_scope.cancel()
    finally:
      bus.terminate()
      bus.wait(timeout=3)
      if bus.stdout is not None:
        bus.stdout.close()
      if bus.stderr is not None:
        bus.stderr.close()


class Native:
  def __init__(self, binary: Path, address: str, output: Path):
    self.log = (output / 'native.stderr').open('w')
    self.process = subprocess.Popen([str(binary.resolve()), address], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, text=True)

  async def call(self, command):
    def exchange():
      assert self.process.stdin is not None and self.process.stdout is not None
      self.process.stdin.write(json.dumps(command) + '\n')
      self.process.stdin.flush()
      assert select.select([self.process.stdout], [], [], 30)[0], 'native BlueZ reply timeout'
      line = self.process.stdout.readline()
      assert line, self.process.poll()
      return json.loads(line)
    return await anyio.to_thread.run_sync(exchange)

  def finish(self):
    if self.process.poll() is None:
      self.process.kill()
    self.process.wait(timeout=3)
    self.log.close()

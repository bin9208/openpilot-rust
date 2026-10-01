import argparse
import hashlib
import json
import os
from pathlib import Path

import anyio
from jeepney.io.asyncio import open_dbus_router
from jeepney.wrappers import DBusErrorResponse
from pytest import MonkeyPatch
from openpilot.selfdrive.carrot.bluetooth import bluez
from bluetooth_bluez_fixture import MAC, PATH, Native, owned_bus


class Source:
  def __init__(self):
    self.client = bluez.Bluez()

  async def call(self, command):
    try:
      match command['op']:
        case 'snapshot':
          result = await self.client.snapshot()
        case 'scan':
          result = await self.client.scan()
        case 'device':
          result = await self.client.device_action(command['address'], command['action'])
        case 'pair':
          result = await self.client.start_pair(command['address'])
        case 'cancel':
          result = await self.client.cancel_pair()
        case 'respond':
          result = self.client.respond(command['id'], command['value'])
        case 'close' | 'reset':
          result = await self.client.close()
        case _:
          raise AssertionError(command)
      return {'result': result}
    except (ValueError, DBusErrorResponse, TimeoutError) as error:
      return {'error': str(error)}


def normalized(value):
  result = json.loads(json.dumps(value))
  if result.get('result') and result['result'].get('prompt'):
    result['result']['prompt']['id'] = '<prompt>'
  return result


async def scenario(binary: Path | None, output: Path):
  await anyio.to_thread.run_sync(lambda: output.mkdir(parents=True, exist_ok=False))
  rows = []
  async with owned_bus() as (address, server):
    with MonkeyPatch.context() as patch:
      patch.setattr(bluez, 'open_dbus_router', lambda _bus: open_dbus_router(address))
      client = Source() if binary is None else Native(binary, address, output)
      try:
        async def call(command):
          response = await client.call(command)
          rows.append({'request': command, 'response': normalized(response)})
          return response

        async def state(wanted):
          with anyio.fail_after(5):
            while True:
              result = await client.call({'op': 'snapshot'})
              if result.get('result', {}).get('pair', {}).get('state') == wanted:
                rows.append({'state': wanted, 'response': normalized(result)})
                return result
              await anyio.sleep(0.005)

        initial = await call({'op': 'snapshot'})
        assert len(initial['result']['adapters']) == 2
        assert initial['result']['devices'][1]['name'] == 'fallback'
        await call({'op': 'scan'})
        await call({'op': 'scan'})
        await call({'op': 'snapshot'})
        for action in ['connect', 'disconnect', 'forget']:
          assert await call({'op': 'device', 'address': MAC.lower(), 'action': action}) == {'result': None}
        await call({'op': 'device', 'address': '00:00:00:00:00:02', 'action': 'connect'})
        await call({'op': 'pair', 'address': 'bad'})
        await call({'op': 'pair', 'address': MAC})
        await state('paired')
        server.mode = 'failure'
        await call({'op': 'pair', 'address': MAC})
        await state('error')
        server.mode = 'connect-failure'
        await call({'op': 'pair', 'address': MAC})
        with anyio.fail_after(5):
          while True:
            result = await client.call({'op': 'snapshot'})
            if result.get('result', {}).get('pair', {}).get('error') == "[org.bluez.Error.Failed] ('connect failed',)":
              rows.append({'post_pair_connect': normalized(result)})
              break
            await anyio.sleep(0.005)
        server.mode = 'hold'
        await call({'op': 'pair', 'address': MAC})
        await state('pairing')
        await call({'op': 'pair', 'address': MAC})
        assert (await server.agent('RequestAuthorization', 'o', ('/wrong',)))['error'] == 'org.bluez.Error.Rejected'
        assert (await server.agent('RequestAuthorization', 'o', (PATH,), interface='org.fixture.Wrong'))['error'] == 'org.bluez.Error.Rejected'
        async with open_dbus_router(address) as attacker:
          assert (await server.agent('Cancel', router=attacker))['error'] == 'org.bluez.Error.Rejected'
        assert (await server.agent('DisplayPasskey', 'ouq', (PATH, 42, 1)))['body'] == []
        displayed = await call({'op': 'snapshot'})
        assert displayed['result']['prompt']['value'] == '000042'
        expired = await client.call({'op': 'respond', 'id': displayed['result']['prompt']['id'], 'value': True})
        rows.append({'display_response': expired})
        assert expired == {'error': 'pairing prompt expired'}
        await server.agent('Release')
        await call({'op': 'cancel'})
        await state('cancelled')
        server.mode = 'confirm'
        await call({'op': 'pair', 'address': MAC})
        with anyio.fail_after(5):
          while True:
            prompted = await client.call({'op': 'snapshot'})
            if prompted.get('result', {}).get('prompt'):
              break
            await anyio.sleep(0.005)
        rows.append({'confirmation': normalized(prompted)})
        prompt_id = prompted['result']['prompt']['id']
        for value in [1, 'yes', True]:
          response = await client.call({'op': 'respond', 'id': prompt_id, 'value': value})
          rows.append({'confirmation_answer': value, 'response': response})
        await state('paired')
        pid = os.getpid() if binary is None else client.process.pid
        async def descriptors():
          return await anyio.to_thread.run_sync(lambda: len(list(Path(f'/proc/{pid}/fd').iterdir())))
        before_reset = await descriptors()
        for _ in range(20):
          assert await call({'op': 'reset'}) == {'result': None}
          await call({'op': 'snapshot'})
        assert await descriptors() == before_reset
        rows.append({'close_reopen_count': 20, 'descriptor_growth': 0})
        server.mode = 'plain'
        assert await call({'op': 'pair', 'address': MAC}) == {'result': None}
        await state('paired')
        assert await call({'op': 'close'}) == {'result': None}
        if binary is not None:
          assert client.process.wait(timeout=3) == 0
      finally:
        if binary is None:
          await client.client.close()
        else:
          client.finish()
        calls = [row for row in server.calls if row['method'] != 'GetManagedObjects']
        result = {'observations': rows, 'calls': calls, 'agent_replies': server.agent_replies}
        (output / 'result.json').write_text(json.dumps(result, indent=2))
  return result


async def run(args):
  source = await scenario(None, args.output / 'source')
  native = await scenario(args.binary, args.output / 'native')
  assert source == native, 'source/native BlueZ observations or ordered calls differ'
  (args.output / 'result.json').write_text(json.dumps({'pass': True, 'observations': len(source['observations']),
    'calls': len(source['calls']), 'agent_replies': len(source['agent_replies']),
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2))
  print('PASS: private D-Bus objects, scan, device actions, pairing outcomes, sender/interface/target checks, prompts and cancellation')


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  arguments = parser.parse_args()
  arguments.output.mkdir(parents=True, exist_ok=False)
  anyio.run(run, arguments)

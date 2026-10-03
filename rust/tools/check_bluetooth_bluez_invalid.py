import argparse
import json
from pathlib import Path

import anyio
from jeepney.io.asyncio import open_dbus_router
from pytest import MonkeyPatch
from openpilot.selfdrive.carrot.bluetooth import bluez
from bluetooth_bluez_fixture import MAC, PATH, Native, owned_bus
from check_bluetooth_bluez import Source


async def scenario(binary, output):
  await anyio.to_thread.run_sync(lambda: output.mkdir(parents=True, exist_ok=False))
  async with owned_bus() as (address, server):
    with MonkeyPatch.context() as patch:
      patch.setattr(bluez, 'open_dbus_router', lambda _bus: open_dbus_router(address))
      client = Source() if binary is None else Native(binary, address, output)
      reply = []
      try:
        server.mode = 'hold'
        assert await client.call({'op': 'pair', 'address': MAC}) == {'result': None}
        async def request():
          try:
            reply.append(await server.agent('RequestPinCode', 'o', (PATH,), reply_seconds=0.5))
          except TimeoutError:
            reply.append({'timeout': True})
        async with anyio.create_task_group() as tasks:
          tasks.start_soon(request)
          with anyio.fail_after(5):
            while True:
              pending = (await client.call({'op': 'snapshot'}))['result']['prompt']
              if pending:
                break
              await anyio.sleep(0.005)
          assert await client.call({'op': 'respond', 'id': pending['id'], 'value': 'a\0b'}) == {'result': None}
        if binary is None:
          assert reply[0]['error'] == 'org.freedesktop.DBus.Error.NoReply', reply
          assert 'disconnected' in reply[0]['body'][0]
        else:
          assert reply == [{'timeout': True}], reply
          assert 'result' in await client.call({'op': 'snapshot'})
          assert await client.call({'op': 'close'}) == {'result': None}
          assert client.process.wait(timeout=3) == 0
      finally:
        if binary is None:
          await client.client.close()
        else:
          client.finish()
        (output / 'result.json').write_text(json.dumps(reply, indent=2))
  return reply


async def run(args):
  source = await scenario(None, args.output / 'source')
  native = await scenario(args.binary, args.output / 'native')
  result = {'pass': True, 'exact_parity': False, 'issue': 166, 'source': source, 'native': native,
            'difference': 'Native suppresses an invalid NUL-containing D-Bus string without disconnecting its session.'}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print('PASS: reproduced inherited invalid-PIN disconnect and verified native bounded no-panic handling; explicit #166 difference')


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  anyio.run(run, parser.parse_args())

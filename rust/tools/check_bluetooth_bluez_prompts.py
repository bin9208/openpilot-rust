import argparse
import hashlib
import json
from pathlib import Path

import anyio
from jeepney.io.asyncio import open_dbus_router
from pytest import MonkeyPatch
from openpilot.selfdrive.carrot.bluetooth import bluez
from bluetooth_bluez_fixture import MAC, PATH, Native, owned_bus
from check_bluetooth_bluez import Source, normalized


async def scenario(binary, output, deadlines):
  await anyio.to_thread.run_sync(lambda: output.mkdir(parents=True, exist_ok=False))
  rows = []
  async with owned_bus() as (address, server):
    with MonkeyPatch.context() as patch:
      patch.setattr(bluez, 'open_dbus_router', lambda _bus: open_dbus_router(address))
      client = Source() if binary is None else Native(binary, address, output)
      try:
        async def prompt(kind):
          with anyio.fail_after(5):
            while True:
              snapshot = await client.call({'op': 'snapshot'})
              value = snapshot.get('result', {}).get('prompt')
              if value and value['kind'] == kind:
                return value
              await anyio.sleep(0.005)

        server.mode = 'hold'
        assert await client.call({'op': 'pair', 'address': MAC}) == {'result': None}
        with anyio.fail_after(5):
          await server.pair_received.wait()
        assert (await server.agent('Unknown', 'o', (PATH,)))['error'] == 'org.bluez.Error.Rejected'
        cases = [
          ('RequestPinCode', 'o', (PATH,), ['한글 PIN', False, 'x' * 16]),
          ('RequestPasskey', 'o', (PATH,), [0, 999999, '１２３４５６', '٠١٢٣', '²', False]),
          ('RequestAuthorization', 'o', (PATH,), [True, False]),
          ('AuthorizeService', 'os', (PATH, '00001124-0000-1000-8000-00805f9b34fb'), [True, False]),
          ('RequestConfirmation', 'ou', (PATH, 42), [True, False]),
        ]
        for kind, signature, body, answers in cases:
          for answer in answers:
            reply = []
            async def request(method=kind, sig=signature, args=body, output_reply=reply):
              output_reply.append(await server.agent(method, sig, args))
            async with anyio.create_task_group() as tasks:
              tasks.start_soon(request)
              pending = await prompt(kind)
              assert await client.call({'op': 'respond', 'id': 'wrong', 'value': True}) == {'error': 'pairing prompt expired'}
              duplicate = await server.agent('RequestAuthorization', 'o', (PATH,))
              assert duplicate['error'] == 'org.bluez.Error.Canceled'
              response = await client.call({'op': 'respond', 'id': pending['id'], 'value': answer})
              assert response == {'result': None}, response
            assert reply
            rows.append({'kind': kind, 'answer': answer, 'value': pending['value'], 'reply': reply[0]})
            assert (await client.call({'op': 'snapshot'}))['result']['prompt'] is None

        for cancel in ['Cancel', 'Release']:
          reply = []
          async def request_cancel(output_reply=reply):
            output_reply.append(await server.agent('RequestAuthorization', 'o', (PATH,)))
          async with anyio.create_task_group() as tasks:
            tasks.start_soon(request_cancel)
            await prompt('RequestAuthorization')
            assert (await server.agent(cancel))['body'] == []
          assert reply[0]['error'] == 'org.bluez.Error.Rejected'
          rows.append({'cancel': cancel, 'reply': reply[0]})

        for answer in ['  +١_٢  ', True, -0.8, -1, 4294967295, 4294967296, 1.5, None, {}, float('nan'), '0' * 4300, '0' * 4301]:
          reply = []
          async def converted(output_reply=reply):
            try:
              output_reply.append(await server.agent('RequestPasskey', 'o', (PATH,), reply_seconds=0.3))
            except TimeoutError:
              output_reply.append({'no_wire_reply': True})
          async with anyio.create_task_group() as tasks:
            tasks.start_soon(converted)
            await prompt('RequestPasskey')
            await server.agent('DisplayPinCode', 'os', (PATH, 'display overrides prompt'))
            displayed = await prompt('DisplayPinCode')
            assert await client.call({'op': 'respond', 'id': displayed['id'], 'value': answer}) == {'result': None}
          rows.append({'display_overlap_passkey': json.dumps(answer), 'reply': reply})

        for kind, signature, body, value in [
          ('DisplayPinCode', 'os', (PATH, '코드'), '코드'),
          ('DisplayPasskey', 'ouq', (PATH, 999999, 6), '999999'),
        ]:
          assert (await server.agent(kind, signature, body))['body'] == []
          assert (await prompt(kind))['value'] == value
          await server.agent('Cancel')

        for answer in ['\ud800']:
          replies = []
          async def no_reply(output_replies=replies):
            try:
              await server.agent('RequestPinCode', 'o', (PATH,), reply_seconds=0.3)
              raise AssertionError('unserializable PIN must not produce a wire reply')
            except TimeoutError:
              output_replies.append('no wire reply')
          async with anyio.create_task_group() as tasks:
            tasks.start_soon(no_reply)
            pending = await prompt('RequestPinCode')
            assert await client.call({'op': 'respond', 'id': pending['id'], 'value': answer}) == {'result': None}
          rows.append({'unserializable_pin': answer, 'reply': replies})

        if deadlines:
          await client.call({'op': 'cancel'})
          server.pending = None
          assert await client.call({'op': 'scan'}) == {'result': None}
          assert await client.call({'op': 'pair', 'address': MAC}) == {'result': None}
          started = anyio.current_time()
          reply = []
          async def expired():
            reply.append(await server.agent('RequestAuthorization', 'o', (PATH,), reply_seconds=65))
          async with anyio.create_task_group() as tasks:
            tasks.start_soon(expired)
            pending = await prompt('RequestAuthorization')
            with anyio.fail_after(35):
              await server.discovery_stopped.wait()
            assert 29 <= anyio.current_time() - started < 35
          assert 59 <= anyio.current_time() - started < 65
          assert reply[0]['error'] == 'org.bluez.Error.Canceled'
          assert await client.call({'op': 'respond', 'id': pending['id'], 'value': True}) == {'error': 'pairing prompt expired'}
          with anyio.fail_after(35):
            while True:
              snapshot = await client.call({'op': 'snapshot'})
              if snapshot.get('result', {}).get('pair', {}).get('state') == 'error':
                break
              await anyio.sleep(0.02)
          assert 89 <= anyio.current_time() - started < 95
          rows.append({'deadlines': normalized(snapshot), 'agent_reply': reply[0]})
        await client.call({'op': 'close'})
      finally:
        if binary is None:
          await client.client.close()
        else:
          client.finish()
        result = {'rows': rows, 'replies': server.agent_replies}
        (output / 'result.json').write_text(json.dumps(result, indent=2))
  return result


async def run(args):
  source = await scenario(None, args.output / 'source', args.deadlines)
  native = await scenario(args.binary, args.output / 'native', args.deadlines)
  assert source == native, 'source/native prompt behavior differs'
  result = {'pass': True, 'cases': len(source['rows']), 'agent_replies': len(source['replies']),
            'real_deadlines': args.deadlines, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print(f'PASS: {result}')


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  parser.add_argument('--deadlines', action='store_true')
  anyio.run(run, parser.parse_args())

import asyncio
import json
import os
from pathlib import Path
import tempfile
import time
from aiohttp import ClientSession
from jeepney.io.asyncio import open_dbus_router
from bluetooth_bluez_fixture import owned_bus, MAC, PATH, DEVICE
from carrot_server_bluetooth_fixture import setup, files, server, write


async def reader_control(args):
  async with owned_bus() as (address, peer):
    process = await asyncio.create_subprocess_exec(str(args.binary), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    started = time.monotonic()
    stdout, stderr = await asyncio.wait_for(process.communicate((json.dumps({'bus': address, 'reader_control': True}) + '\n').encode()), 5)
    result = json.loads(stdout)
    result.update(returncode=process.returncode, stderr=stderr.decode(), elapsed=time.monotonic() - started, calls=peer.calls)
    result['pass'] = process.returncode == 0 and result['before'] == result['after'] and result['old_reader_error'] == 'Bluetooth D-Bus connection closed' and [call['method'] for call in peer.calls] == ['GetManagedObjects', 'GetManagedObjects']
    write(args.output / 'result.json', result)
    print(json.dumps(result))
    return 0 if result['pass'] else 1


async def timeout_scenario(native, args):
  output = args.output / ('native' if native else 'source')
  output.mkdir(parents=True, exist_ok=True)
  rows, elapsed = [], []
  with tempfile.TemporaryDirectory(prefix='bluetooth-timeouts-') as temporary:
    root = Path(temporary)
    command = setup(root)
    old_path = os.environ.get('PATH', '')
    os.environ['PATH'] = str(command.parent) + os.pathsep + old_path
    try:
      async with owned_bus() as (address, peer), server(native, args.binary, root, address, command, output) as port, ClientSession() as client:
        async def request(name, operation=None, limit=25):
          started = time.monotonic()
          try:
            async with asyncio.timeout(limit):
              method = 'POST' if operation else 'GET'
              async with client.request(method, f'http://127.0.0.1:{port}/api/bluetooth' + ('/' + operation if operation else ''), json={'enabled': True} if operation else None) as response:
                data = await response.read()
                result = {'name': name, 'status': response.status, 'body_hex': data.hex()}
                if operation is None: result['json'] = json.loads(data)
          except TimeoutError:
            result = {'name': name, 'client_timed_out': True}
          rows.append(result)
          elapsed.append({'name': name, 'elapsed': time.monotonic() - started})
          return result
        write(root / 'command-control.json', {'hold': 'test'})
        await request('radio-indicator-timeout')
        write(root / 'command-control.json', {'hold': 'mkdir'})
        mutation = asyncio.create_task(request('radio-operation-timeout', 'radio'))
        async with asyncio.timeout(5):
          while not any(row['args'][1] == 'mkdir' for row in [json.loads(line) for line in (root / 'command-pids.jsonl').read_text().splitlines()]):
            await asyncio.sleep(.01)
        await request('status-during-radio-command', limit=.5)
        await mutation
        write(root / 'command-control.json', {})
        await request('radio-operation-recovered', 'radio')
        await request('radio-indicator-recovered')
      survivors = []
      for row in [json.loads(line) for line in (root / 'command-pids.jsonl').read_text().splitlines()]:
        try: os.kill(row['pid'], 0)
        except ProcessLookupError: continue
        survivors.append(row)
      write(output / 'observations.json', rows)
      write(output / 'elapsed.json', elapsed)
      write(output / 'process-survivors.json', survivors)
      assert not survivors, survivors
    finally:
      os.environ['PATH'] = old_path
  return rows


async def composed_scenario(native, args):
  output = args.output / ('native' if native else 'source')
  output.mkdir(parents=True, exist_ok=True)
  rows = []
  with tempfile.TemporaryDirectory(prefix='bluetooth-application-') as temporary:
    root = Path(temporary)
    command = setup(root)
    old_path = os.environ.get('PATH', '')
    os.environ['PATH'] = str(command.parent) + os.pathsep + old_path
    try:
      async with owned_bus() as (address, peer):
        async with server(native, args.binary, root, address, command, output, composed=True) as port, ClientSession() as client:
          async def request(name, operation=None, body=None, method=None, headers=None, raw=None):
            async with client.request(method or ('POST' if operation else 'GET'), f'http://127.0.0.1:{port}/api/bluetooth' + ('/' + operation if operation else ''), json=(body or {}) if operation and raw is None else None, data=raw, headers=headers) as response:
              data = await response.read()
              row = {'name': name, 'status': response.status, 'headers': {key.lower(): value for key, value in response.headers.items() if key.lower() in ('content-type', 'content-length', 'allow') or (name == 'application-body-read-error' and key.lower() == 'connection')}, 'body_hex': data.hex(), 'files': files(root)}
              rows.append(row)
              write(output / 'partial-observations.json', rows)
              return row
          await request('application-status')
          await request('application-head', method='HEAD')
          await request('application-method', 'scan', method='GET')
          await request('application-origin', 'connect', {'address': MAC}, headers={'Origin': 'https://outside.invalid'})
          await request('application-body-read-error', 'unknown', headers={'Content-Type': 'application/json', 'Content-Encoding': 'gzip'}, raw=b'badgzip')
          await request('application-connect', 'connect', {'address': MAC})
          peer.objects[PATH][DEVICE]['Address'] = ('s', MAC)
          peer.objects[PATH][DEVICE]['Paired'] = ('b', True)
          await request('application-config', 'config', {'devices': {MAC: {'profile': 'generic', 'enabled': True}}})
          await request('application-learn', 'learn', {'address': MAC, 'enabled': True})
          await request('application-disconnect', 'disconnect', {'address': MAC})
          await request('application-status-after-writes')
          peer.mode = 'hold'
          await request('application-pair-before-cleanup', 'pair', {'address': MAC})
          await asyncio.wait_for(peer.pair_received.wait(), 5)
        write(output / 'private-bus.json', peer.calls)
        write(output / 'observations.json', rows)
        assert peer.calls[-1]['method'] == 'CancelPairing', peer.calls
    finally:
      os.environ['PATH'] = old_path
  return rows


async def direct_consumer(args):
  async with owned_bus() as (address, peer):
    from openpilot.selfdrive.carrot.bluetooth import bluez
    bluez.open_dbus_router = lambda _bus: open_dbus_router(address)
    client = bluez.Bluez()
    source_snapshot = await client.snapshot()
    await client.scan()
    if not args.immediate_scan_close:
      await asyncio.sleep(0)
    await client.cancel_pair()
    await client.close()
    source_calls = peer.calls.copy()
  async with owned_bus() as (address, peer):
    process = await asyncio.create_subprocess_exec(str(args.consumer), address, stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    commands = [{'op': op} for op in ('snapshot', 'scan', 'cancel', 'close')]
    if args.immediate_scan_close:
      stdout, stderr = await asyncio.wait_for(process.communicate((''.join(json.dumps(command) + '\n' for command in commands)).encode()), 10)
      results = [json.loads(line) for line in stdout.splitlines()]
    else:
      results = []
      for command in commands:
        process.stdin.write((json.dumps(command) + '\n').encode())
        await process.stdin.drain()
        results.append(json.loads(await asyncio.wait_for(process.stdout.readline(), 5)))
      stdout, stderr = await asyncio.wait_for(process.communicate(), 10)
      assert not stdout, stdout
    result = {'source_snapshot': source_snapshot, 'source_calls': source_calls, 'native_results': results, 'native_calls': peer.calls, 'returncode': process.returncode, 'stderr': stderr.decode()}
    result['pass'] = process.returncode == 0 and results == [{'result': source_snapshot}, {'result': None}, {'result': None}, {'result': None}] and source_calls == peer.calls
    write(args.output / 'result.json', result)
    print(json.dumps(result))
    return 0 if result['pass'] else 1

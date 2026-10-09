import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
from aiohttp import ClientSession
from jeepney import HeaderFields, new_error
from bluetooth_bluez_fixture import owned_bus, MAC, PATH, DEVICE
from carrot_server_bluetooth_fixture import setup, files, server, write
from carrot_server_bluetooth_controls import reader_control, timeout_scenario, composed_scenario, direct_consumer


async def scenario(native, args):
  label = 'native' if native else 'source'
  output = args.output / label
  output.mkdir(parents=True, exist_ok=True)
  rows = []
  with tempfile.TemporaryDirectory(prefix='bluetooth-http-') as temporary:
    root = Path(temporary)
    command = setup(root)
    previous_path = os.environ.get('PATH', '')
    os.environ['PATH'] = str(command.parent) + os.pathsep + previous_path
    try:
      async with owned_bus() as (address, peer):
        held, release = asyncio.Event(), asyncio.Event()
        control = {'hold': False, 'error': False}
        original = peer.dispatch
        async def dispatch(message):
          method = message.header.fields.get(HeaderFields.member)
          if method == 'Connect' and control['hold']:
            held.set()
            await release.wait()
          if method == 'GetManagedObjects' and control['error']:
            peer.calls.append({'method': method, 'body': message.body})
            await peer.router.send(new_error(message, 'org.bluez.Error.Failed', 's', ('owned object failure',)))
            return
          await original(message)
        peer.dispatch = dispatch
        async with server(native, args.binary, root, address, command, output) as port, ClientSession() as client:
          async def request(name, operation=None, body=None, method=None, headers=None, raw=None):
            path = '/api/bluetooth' + ('/' + operation if operation is not None else '')
            method = method or ('POST' if operation is not None else 'GET')
            actual = {'Content-Type': 'application/json'}
            actual.update(headers or {})
            if actual.get('Origin') == 'SAME': actual['Origin'] = f'http://127.0.0.1:{port}'
            payload = raw if raw is not None else json.dumps(body if body is not None else {}).encode()
            async with client.request(method, f'http://127.0.0.1:{port}{path}', data=payload if method == 'POST' else None, headers=actual) as response:
              data = await response.read()
              row = {'name': name, 'status': response.status, 'headers': {key.lower(): value for key, value in response.headers.items() if key.lower() in ('content-type', 'content-length', 'allow')}, 'body_hex': data.hex(), 'files': files(root)}
              if data and response.content_type == 'application/json': row['json'] = json.loads(data)
              rows.append(row)
              return row
          await request('initial')
          await request('head', method='HEAD')
          await request('status-post', method='POST')
          await request('mutation-get', 'scan', method='GET')
          await request('mutation-head', 'scan', method='HEAD')
          await request('origin-reject', 'scan', headers={'Origin': 'https://outside.invalid'})
          await request('origin-no-authority', 'scan', headers={'Origin': 'null'})
          await request('fetch-site-reject', 'scan', headers={'Sec-Fetch-Site': 'cross-site'})
          await request('content-reject', 'scan', headers={'Content-Type': 'text/plain'})
          for name, state in [('missing', None), ('array', []), ('bad-time', {'time': '100', 'stationary': True}), ('stale-edge', {'time': 98., 'stationary': True}), ('future', {'time': 100.0001, 'stationary': True}), ('stopped', {'time': 100, 'stationary': True, 'stopped': True}), ('moving', {'time': 100, 'stationary': False}), ('fresh-edge', {'time': 98.0001, 'stationary': [1]})]:
            if state is None: (root / 'runtime/status.json').unlink()
            else: write(root / 'runtime/status.json', state)
            await request('runtime-' + name)
            await request('guard-' + name, 'unknown')
          write(root / 'runtime/status.json', {'time': 100, 'stationary': True})
          for name, raw in [('empty', b''), ('invalid', b'{'), ('array', b'[]'), ('null', b'null'), ('utf8-bom', b'\xef\xbb\xbf{}'), ('utf16', '{}'.encode('utf-16')), ('utf32', '{}'.encode('utf-32')), ('exact-limit', b'{}' + b' ' * 32766), ('over-limit', b'{}' + b' ' * 32767)]:
            await request('body-' + name, 'unknown', raw=raw)
          await request('body-decode-read-error', 'unknown', raw=b'badgzip', headers={'Content-Encoding': 'gzip'})
          for name, value in [('absent', None), ('invalid', 'bad'), ('integer', 123), ('lowercase', MAC.lower()), ('missing-device', '00:00:00:00:00:99')]:
            await request('address-' + name, 'connect', {'address': value})
          peer.mode = 'connect-failure'
          await request('connect-error', 'connect', {'address': MAC})
          peer.mode = 'plain'
          await request('connect-recovery', 'connect', {'address': MAC}, headers={'Origin': 'SAME'})
          control['error'] = True
          await request('objects-error')
          await request('invalid-address-before-bus', 'pair', {'address': 'bad'})
          await request('objects-mutation-error', 'connect', {'address': MAC})
          control['error'] = False
          await request('objects-recovery')
          await request('answer-expired', 'answer', {'id': None, 'value': True})
          await request('cancel-idle', 'cancel')
          await request('learn-unsaved', 'learn', {'address': MAC, 'enabled': True})
          device = {'name': '한글', 'profile': 'generic', 'enabled': True, 'mapping': {'up': 'accelCruise', 'key:767': 'none'}}
          await request('config-unpaired', 'config', {'devices': {MAC: device}})
          peer.objects[PATH][DEVICE]['Paired'] = ('b', True)
          await request('config-case-sensitive-paired', 'config', {'devices': {MAC: device}})
          peer.objects[PATH][DEVICE]['Address'] = ('s', MAC)
          await request('config-saved', 'config', {'devices': {MAC: device}})
          await request('config-identical', 'config', {'devices': {MAC: device}})
          await request('learn-enabled-type', 'learn', {'address': MAC, 'enabled': 1})
          await request('learn-on', 'learn', {'address': MAC, 'enabled': True})
          await request('learn-off', 'learn', {'address': MAC, 'enabled': False})
          for name, proposed in [('devices-type', {'devices': []}), ('profile', {'devices': {MAC: {'profile': 'unknown'}}}), ('mapping', {'devices': {MAC: dict(device, mapping={'key:768': 'none'})}}), ('enabled', {'devices': {MAC: dict(device, enabled=1)}}), ('too-many', {'devices': {f'00:00:00:00:00:{index:02x}': device for index in range(17)}})]:
            await request('config-' + name, 'config', proposed)
          await request('device-config-changed', 'device-config', {'address': MAC, 'device': dict(device, name='changed')})
          await request('disconnect', 'disconnect', {'address': MAC})
          await request('forget', 'forget', {'address': MAC})
          await request('config-after-forget')
          write(root / 'runtime/cancelled.json', [])
          await request('cancelled-invalid-recovery', 'disconnect', {'address': MAC})
          peer.mode = 'hold'
          await request('pair-start', 'pair', {'address': MAC})
          await asyncio.wait_for(peer.pair_received.wait(), 5)
          await request('pair-held-status')
          await request('pair-already-active', 'pair', {'address': MAC})
          await request('pair-cancel', 'cancel')
          await request('pair-cancelled-status')
          peer.mode = 'plain'
          await request('scan', 'scan')
          await request('scan-already-active', 'scan')
          await request('scan-status')
          await request('radio-type', 'radio', {'enabled': 1})
          await request('radio-enable', 'radio', {'enabled': True})
          await request('radio-enabled-status')
          write(root / 'command-control.json', {'fail': 'systemctl', 'error': ('owned error 한글' * 60).encode().hex()})
          await request('radio-failure', 'radio', {'enabled': False})
          write(root / 'command-control.json', {})
          await request('radio-disable-recovery', 'radio', {'enabled': False})
          await request('radio-disabled-status')
          control['hold'] = True
          pending = asyncio.create_task(request('held-connect', 'connect', {'address': MAC}))
          await asyncio.wait_for(held.wait(), 5)
          status = await asyncio.wait_for(request('status-during-held-connect'), .5)
          assert status['status'] == 200 and not pending.done()
          queued = asyncio.create_task(request('queued-learn', 'learn', {'address': MAC, 'enabled': True}))
          await asyncio.sleep(.05)
          assert not queued.done()
          write(root / 'runtime/status.json', {'time': 100, 'stationary': False})
          release.set()
          await pending
          await queued
          write(root / 'runtime/status.json', {'time': 100, 'stationary': True})
          control['hold'] = False
          await request('stationary-recovered', 'config', {})
          reader, writer = await asyncio.open_connection('127.0.0.1', port)
          writer.write(f'POST /api/bluetooth/unknown HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 20\r\n\r\n{{}}'.encode())
          await writer.drain()
          headers = await asyncio.wait_for(reader.readuntil(b'\r\n\r\n'), .5)
          length = next(int(line.split(b':', 1)[1]) for line in headers.split(b'\r\n') if line.lower().startswith(b'content-length:'))
          body = await reader.readexactly(length)
          rows.append({'name': 'single-read-before-body-completion', 'status': int(headers.split()[1]), 'body_hex': body.hex(), 'files': files(root)})
          writer.close()
          await writer.wait_closed()
        write(output / 'private-bus.json', peer.calls)
        write(output / 'commands.json', [json.loads(line) for line in (root / 'commands.jsonl').read_text().splitlines()])
    finally:
      os.environ['PATH'] = previous_path
  write(output / 'observations.json', rows)
  return rows


async def main(args):
  args.output.mkdir(parents=True, exist_ok=True)
  if args.reader_only: return await reader_control(args)
  if args.consumer: return await direct_consumer(args)
  if args.composed_only:
    source = await composed_scenario(False, args)
    native = await composed_scenario(True, args)
    write(args.output / 'result.json', {'pairs': len(source), 'pass': source == native, 'source': source, 'native': native})
    print(json.dumps({'pairs': len(source), 'pass': source == native}))
    return 0 if source == native else 1
  if args.timeouts_only:
    source = await timeout_scenario(False, args)
    native = await timeout_scenario(True, args)
    source.sort(key=lambda row: row['name'])
    native.sort(key=lambda row: row['name'])
    write(args.output / 'result.json', {'pairs': len(source), 'pass': source == native, 'source': source, 'native': native})
    print(json.dumps({'pairs': len(source), 'pass': source == native}))
    return 0 if source == native else 1
  source = await scenario(False, args)
  native = await scenario(True, args)
  failures = []
  assert len(source) == len(native)
  for left, right in zip(source, native):
    if left != right: failures.append({'name': left['name'], 'source': left, 'native': right})
  commands_equal = json.loads((args.output / 'source/commands.json').read_text()) == json.loads((args.output / 'native/commands.json').read_text())
  write(args.output / 'failures.json', failures)
  result = {'http_pairs': len(source), 'observations': len(source) + len(native), 'failures': len(failures), 'owned_commands_equal': commands_equal, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  write(args.output / 'result.json', result)
  print(json.dumps(result))
  return 0 if not failures and commands_equal else 1


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--reader-only', action='store_true')
  parser.add_argument('--timeouts-only', action='store_true')
  parser.add_argument('--composed-only', action='store_true')
  parser.add_argument('--consumer', type=Path)
  parser.add_argument('--immediate-scan-close', action='store_true')
  arguments = parser.parse_args()
  sys.exit(asyncio.run(main(arguments)))

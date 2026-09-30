#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace

import zmq
from check_dashcam_metadata import environment, source
from check_params_string import receive
from original_params_binding import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  repository = Path(__file__).resolve().parents[2]
  reports = []
  with tempfile.TemporaryDirectory(prefix='dashcam-params-') as temporary, zmq.Context() as context:
    root = Path(temporary)
    os.environ['OPENPILOT_PREFIX'] = 'fixture'
    with context.socket(zmq.PULL) as source_socket, context.socket(zmq.PULL) as native_socket:
      endpoints = ['ipc://' + str(root / name) for name in ['source-log', 'native-log']]
      for socket, endpoint in zip([source_socket, native_socket], endpoints, strict=True):
        socket.setsockopt(zmq.RCVTIMEO, 5000)
        socket.bind(endpoint)
      binding, swaglog = load(args.binding.resolve(), endpoints[0], root / 'logs')
      params = binding.Params(str(root / 'source'))
      original = source(repository, root / 'settings.json')
      with (args.output / 'process.log').open('w') as log:
        with subprocess.Popen(
          [str(args.binary.resolve()), str(root / 'native'), 'fixture', endpoints[1]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, text=True
        ) as native:
          assert json.loads(native.stdout.readline()) == {'initialized': True}
          keys = [
            'CarName',
            'DongleId',
            'HardwareSerial',
            'DeviceSerial',
            'Serial',
            'CarrotSerial',
            'CarrotDiscordWebhookUrl',
            'CarrotDiscordWebhookURL',
            'DiscordWebhookUrl',
            'DiscordWebhookURL',
          ]
          inputs = [None, b'', b' ordinary ', '\x1c한글 😀\x1f'.encode(), b'\xff', b"a'\xff", b'\xe2\x82', b'\0']
          cases = []
          for key in keys:
            for data in inputs:
              cases.append(({'op': 'param', 'key': key, 'default': 'fallback'}, {key: data}))
          for env, hardware in [
            ({}, ''),
            ({}, ' hw '),
            ({'SERIAL': ' third '}, ''),
            ({'DEVICE_SERIAL': ' second ', 'SERIAL': 'ignored'}, ''),
            ({'CARROT_DEVICE_SERIAL': '\x1cfirst\x1f', 'DEVICE_SERIAL': 'ignored'}, 'ignored'),
          ]:
            cases.append(({'op': 'serial', 'environment': env, 'serial': hardware}, {}))
          for raw in [None, b'car', b'\xff']:
            cases.append(
              (
                {'op': 'metadata', 'environment': {'CARROT_DEVICE_SERIAL': 'fixture-serial'}, 'serial': '', 'repo': str(repository)},
                {'CarName': raw, 'DongleId': raw},
              )
            )
          for env in [
            {'CARROT_DISCORD_WEBHOOK_DISABLE': '1'},
            {'CARROT_DISCORD_WEBHOOK_DISABLE': '\x1cYeS\x1f'},
            {'CARROT_DISCORD_WEBHOOK_URL': ' http://fixture/primary ', 'DISCORD_WEBHOOK_URL': 'http://ignored', 'CARROT_DISCORD_WEBHOOK_DISABLE': '1'},
            {'CARROT_DISCORD_WEBHOOK_URL': ' ', 'DISCORD_WEBHOOK_URL': '\x1chttp://fixture/secondary\x1f'},
          ]:
            cases.append(({'op': 'webhook', 'environment': env}, {}))
          cases.append(({'op': 'param', 'key': 'DongleId', 'default': 'fallback', 'close': True}, {'DongleId': b'\xff'}))
          for index, (request, files) in enumerate(cases):
            for kind in ['source', 'native']:
              directory = root / kind / 'fixture'
              for key in keys:
                path = directory / key
                if path.exists():
                  path.unlink()
              for key, data in files.items():
                if data is not None:
                  (directory / key).write_bytes(data)
            serial = request.get('serial', '')
            original['HARDWARE'] = SimpleNamespace(get_serial=lambda serial=serial: serial)
            if request.get('close'):
              swaglog.ipchandler.sock.close()
            env = request.get('environment', {}) | {'CARROT_REPO_DIR': request.get('repo', str(repository))}
            with environment(env, clear=True):
              match request['op']:
                case 'param':
                  expected = original['param_text'](params, request['key'], request['default'])
                case 'serial':
                  expected = original['device_serial'](params)
                case 'metadata':
                  expected = original['upload_metadata'](params)
                case 'webhook':
                  expected = original['discord_webhook_url'](params)
                case unexpected:
                  raise AssertionError(unexpected)
            marker = f'dashcam-params-{index}'
            source_records = []
            if not request.get('close'):
              swaglog.cloudlog.debug(marker)
              source_records = receive(source_socket, marker)
            native.stdin.write(json.dumps(request | {'marker': marker}) + '\n')
            native.stdin.flush()
            actual = json.loads(native.stdout.readline())
            native_records = [] if request.get('close') else receive(native_socket, marker)
            assert actual == expected, (request, actual, expected)
            assert [row['record']['msg'] for row in native_records] == [row['record']['msg'] for row in source_records], request
            reports.append(
              {
                'request': request,
                'input_hex': {key: None if value is None else value.hex() for key, value in files.items()},
                'value': actual,
                'source_records': source_records,
                'native_records': native_records,
              }
            )
          native.stdin.close()
          assert native.wait(timeout=5) == 0
  (args.output / 'report.json').write_text(json.dumps({'passed': True, 'cases': reports}, indent=2) + '\n')
  print(json.dumps({'passed': len(reports)}))


if __name__ == '__main__':
  main()

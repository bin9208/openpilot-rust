#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
from types import SimpleNamespace
from athena_reference import source, ParamsStore


def run_source(scope, text, binary=False):
  end = threading.Event()
  outputs = []

  def reply(value):
    outputs.append({'reply': json.loads(value)})
    end.set()

  def log(value):
    outputs.append({'log': value})
    end.set()

  scope['send_queue'] = SimpleNamespace(put_nowait=reply)
  scope['log_recv_queue'] = SimpleNamespace(put_nowait=log)
  scope['recv_queue'].put(text.encode() if binary else text)
  scope['jsonrpc_handler'](end)
  return outputs[0]


def requests():
  messages = ['bad', '{"method":', '{}', '[]', 'null', '{"result":1,"id":9}', '{"error":null,"id":"old"}']
  for method, params in [
    ('echo', [None]), ('echo', [{'unicode': '한글😀', 'empty': []}]), ('echo', []), ('echo', [1, 2]), ('echo', {'s': 'named'}), ('echo', {'wrong': 3}),
    ('getMessage', [None]), ('getMessage', ['missing']), ('getVersion', []), ('getVersion', [1]), ('listUploadQueue', []),
    ('cancelUpload', ['missing']), ('cancelUpload', [['missing', 'also-missing']]), ('setRouteViewed', ['abc']), ('setRouteViewed', ['abc']),
    ('getPublicKey', []), ('getSshAuthorizedKeys', []), ('getGithubUsername', []), ('getSimInfo', []), ('getNetworkType', []), ('getNetworkMetered', []), ('getNetworks', []),
    ('startLocalProxy', ['ws://127.0.0.1:1', 23]), ('uploadFileToUrl', []), ('uploadFilesToUrls', [[]]), ('listDataDirectory', []), ('listDataDirectory', ['folder']),
    ('missing', []),
  ]:
    messages.append(json.dumps({'jsonrpc': '2.0', 'id': len(messages), 'method': method, 'params': params}))
  messages.extend([
    '{"method":"echo","params":[1],"id":1}',
    '{"method":"echo","params":[],"id":null}',
    '{"method":"echo","params":[1],"id":null}',
    '{"method":"missing","params":[],"id":null}',
    '{"method":"echo","params":null,"id":1}',
    '{"method":1,"params":[],"id":1}',
    '{"jsonrpc":"wrong-version","method":"echo","params":[1],"id":true}',
    '{"jsonrpc":"2.0","method":"echo","params":[NaN],"id":100}',
    '{"jsonrpc":"2.0","method":"echo","params":["\\ud800"],"id":101}',
    '{"jsonrpc":"2.0","method":"echo","params":[1]}',
    '{"jsonrpc":"2.0","method":"missing","id":null}',
    '{"jsonrpc":"2.0","method":"rpc.reserved","id":1}',
    '{"jsonrpc":"2.0","method":"echo","params":9,"id":1}',
    '{"jsonrpc":"2.0","method":"echo","params":[],"id":1.5}',
    '[{"jsonrpc":"2.0","method":"echo","params":["a"],"id":1},{"jsonrpc":"2.0","method":"echo","params":["b"]}]',
    '[{"jsonrpc":"2.0","method":"echo","id":1},1]',
    '{"jsonrpc":"2.0","method":"echo","extra":1,"id":1}',
  ])
  return [{'data': text, 'binary': False} for text in messages] + [{'data': '{"method":"echo"}', 'binary': True}]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  with tempfile.TemporaryDirectory(prefix='athena-rpc-') as temp:
    root = Path(temp)
    logs = root / 'data'
    logs.mkdir()
    (logs / 'file').write_text('synthetic')
    (logs / 'folder').mkdir()
    (logs / 'folder/child').write_text('synthetic')
    metadata = {'channel': 'test', 'openpilot': {'version': '1.2.3', 'git_origin': 'git@github.com:synthetic/athena.git', 'git_commit': '0' * 40}}
    (root / 'build.json').write_text(json.dumps(metadata))
    params = ParamsStore()
    scope = source(logs, params)
    scope['get_build_metadata'] = lambda: SimpleNamespace(channel='test', openpilot=SimpleNamespace(version='1.2.3', git_normalized_origin='github.com/synthetic/athena', git_commit='0' * 40))
    scope['get_key_pair'] = lambda: (None, None, None)
    scope['HARDWARE'] = SimpleNamespace(get_sim_info=lambda: {'sim_id': '', 'mcc_mnc': None, 'network_type': ['Unknown'], 'sim_state': ['ABSENT'], 'data_connected': False}, get_network_type=lambda: 1, get_network_metered=lambda _: False, get_networks=lambda: None)
    rows = requests()
    expected = [run_source(scope, row['data'], row['binary']) for row in rows]
    env = dict(os.environ, PARAMS_ROOT=str(root / 'params'), OPENPILOT_PREFIX=root.name, LOG_ROOT=str(logs), OPENPILOT_BASEDIR=str(root))
    process = subprocess.run([args.binary], input=''.join(json.dumps(row) + '\n' for row in rows), env=env, text=True, capture_output=True, check=True)
    actual = [json.loads(line) for line in process.stdout.splitlines()]
    records = []
    for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
      equivalent = json.dumps(left, sort_keys=True) == json.dumps(right, sort_keys=True)
      records.append({'request': rows[index], 'source': left, 'native': right, 'pass': equivalent})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(records, indent=2) + '\n')
  failures = [index for index, row in enumerate(records) if not row['pass']]
  assert not failures, failures
  print(f'PASS: {len(records)} unchanged-source JSON-RPC, routing, metadata/hardware, Params and filesystem scenarios')


if __name__ == '__main__':
  main()

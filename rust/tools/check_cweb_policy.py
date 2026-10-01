#!/usr/bin/env python3
import argparse
import ast
import contextlib
import io
from ipaddress import ip_address
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/carrot/cweb_push.py'
FIELDS = ['last_success_ip', 'current_candidate_ip', 'current_candidate_since', 'was_down', 'first_report', 'next_retry_at', 'backoff_s', 'next_heartbeat_at']


def module():
  tree = ast.parse(SOURCE.read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.Assign, ast.ClassDef, ast.FunctionDef))]
  scope = {'__name__': 'cweb_oracle', 'Params': object, 'Any': object, 'ip_address': ip_address, 'json': json, 'socket': None,
           'time': SimpleNamespace(), 'random': SimpleNamespace()}
  exec(compile(tree, str(SOURCE), 'exec'), scope)
  return scope


def expected(request):
  scope = module()
  state = {'now': request['start'], 'fraction': request['fraction']}
  scope['time'] = SimpleNamespace(monotonic=lambda: state['now'], time=lambda: state['wall'])
  scope['random'] = SimpleNamespace(uniform=lambda low, high: low + (high-low)*state['fraction'])
  scope['get_local_ip'] = lambda iface: state['ip']
  scope['device_id'] = lambda params: state['id']
  calls = []
  def post(url, payload, timeout):
    calls.append({'url': url, 'payload': payload, 'timeout': timeout})
    value = state['result']
    return value['ok'], value['status'], value['body']
  scope['post_json'] = post
  reporter = scope['CwebPushReporter'](params=None, **request['config'])
  def snapshot():
    return {key: getattr(reporter, key) for key in FIELDS}
  initial, rows = snapshot(), []
  for frame in request['frames']:
    state.update(frame)
    with contextlib.redirect_stdout(io.StringIO()) as output:
      result = reporter.poll_once()
    statuses = [json.loads(line.removeprefix('[cweb_push] ')) for line in output.getvalue().splitlines()]
    rows.append({'result': result, 'state': snapshot(), 'statuses': statuses, 'calls': list(calls)})
    calls.clear()
  helpers = [{'strip': text.strip(), 'ip': scope['_usable_ip'](text), 'id': scope['_meaningful_device_id'](text),
              'heartbeat': scope['_default_heartbeat_url'](text), 'notify': scope['_default_notify_url'](text)} for text in request['helpers']]
  return {'initial': initial, 'rows': rows, 'helpers': helpers}


def cases():
  rng = random.Random(142)
  helpers = ['', ' unknown ', 'UnregisteredDevice', ' NONE ', 'null', '한글', '0.0.0.0', '127.0.1.1', '169.254.1.2', '224.1.1.1',
             '255.255.255.255', '0.1.2.3', '192.168.001.1', '10.1.2.3', '::1', '10.1.2.3\x1c', 'https://fixture/report', 'https://fixture///']
  for dry_run in [False, True]:
    for heartbeat in [0., 5., 10., 30.]:
      for debounce in [-1., 0., 1., 4.]:
        now, frames = 100., []
        for index in range(500):
          now += rng.choice([0., .9999, 1., 1.0001, 4., 5., 10., 180.])
          frames.append({'now': now, 'wall': 1720000000.9 + now, 'ip': rng.choice(['', '192.168.1.2', '192.168.1.2', '10.1.2.3']),
                         'id': 'fixture-한글', 'fraction': rng.random(),
                         'result': {'ok': index % 7 == 0, 'status': 200 if index % 7 == 0 else 503, 'body': '한글🙂' * 100}})
        yield {'config': {'report_url': 'http://fixture/report', 'heartbeat_url': 'http://fixture/heartbeat', 'iface': 'wlan0', 'port': -7,
                          'timeout_s': 4., 'heartbeat_interval_s': heartbeat, 'debounce_s': debounce, 'dry_run': dry_run},
               'start': 100., 'fraction': .3, 'frames': frames, 'helpers': helpers}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  requests = list(cases())
  run = subprocess.run([args.binary.resolve()], input=''.join(json.dumps(case) + '\n' for case in requests), text=True, capture_output=True, check=True)
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  args.output.mkdir(parents=True, exist_ok=False)
  assert len(actual) == len(requests)
  for index, (request, value) in enumerate(zip(requests, actual, strict=True)):
    source = expected(request)
    if source != value:
      (args.output / 'failure.json').write_text(json.dumps({'case': index, 'source': source, 'native': value}, ensure_ascii=False, indent=2))
      raise AssertionError(('source/native mismatch', index))
  result = {'cases': len(requests), 'frames': sum(len(case['frames']) for case in requests), 'exact': True}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

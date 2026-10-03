from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

from check_navd_http import Server
from navd_cases import parameters, route
from openpilot.cereal import log, messaging

ROOT = Path(__file__).resolve().parents[2]


def message(service, value):
  event = messaging.new_message(service, valid=True)
  setattr(event, service, value)
  return event


class Peer:
  def __init__(self, implementation, scenario, args, output):
    self.output = output
    self.stack = ExitStack()
    shm = Path(self.stack.enter_context(tempfile.TemporaryDirectory(prefix='msgq_navd_', dir='/dev/shm')))
    self.prefix = shm.name.removeprefix('msgq_')
    os.environ['OPENPILOT_PREFIX'] = self.prefix
    self.publisher = messaging.PubMaster(['carrotMan', 'managerState'])
    self.subscribers = {name: messaging.sub_sock(name, conflate=False, timeout=20) for name in ('navInstruction', 'navRouteNavd')}
    self.params = output / 'params' / self.prefix
    self.params.mkdir(parents=True)
    initial = parameters()
    initial.pop('NavDestination')
    initial['PrimeType'] = '0'
    for key, value in initial.items():
      self.put(key, value)
    self.server = Server([])
    host = self.server.url.split('/route?')[0]
    env = dict(os.environ, PARAMS_ROOT=str(self.params.parent), MAPBOX_TOKEN='fixture-token',
               OPENPILOT_ROOT=str(ROOT), LOGPRINT='info')
    if implementation == 'source':
      command = [sys.executable, str(ROOT / 'rust/tools/navd_runtime_source.py'), '--binding', str(args.binding.resolve()),
                 '--host', host, '--output', str(output)]
    else:
      command = [*args.runner, str(args.binary.resolve()), '--mapbox-host', host]
    self.command = command
    self.stdout = (output / 'stdout.log').open('w')
    self.stderr = (output / 'stderr.log').open('w')
    self.process = subprocess.Popen(command, env=env, stdout=self.stdout, stderr=self.stderr)
    self.rows = []
    self.ui_pid = 10
    self.pending = []
    self.scenario = scenario

  def put(self, key, value):
    path = self.params / key
    if value is None:
      path.unlink(missing_ok=True)
    else:
      temporary = path.with_suffix('.new')
      temporary.write_text(value)
      temporary.replace(path)

  def pump(self):
    assert self.process.poll() is None, ('daemon exited', self.process.returncode, self.output)
    self.publisher.send('carrotMan', message('carrotMan', {'xPosLat': 37., 'xPosLon': 127., 'xPosAngle': 359.5}))
    self.publisher.send('managerState', message('managerState', {'processes': [{'name': 'ui', 'pid': self.ui_pid, 'running': True}]}))
    for service, socket in self.subscribers.items():
      while (raw := socket.receive(non_blocking=True)) is not None:
        with (self.output / f'{service}.bin').open('ab') as stream:
          stream.write(raw)
        with log.Event.from_bytes(raw) as event:
          row = {'service': service, 'valid': event.valid, 'data': getattr(event, service).to_dict(), 'time': time.monotonic()}
        self.rows.append(row)
        self.pending.append(row)
    time.sleep(.01)

  def wait(self, predicate, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      self.pump()
      for index, row in enumerate(self.pending):
        if predicate(row):
          self.pending.pop(index)
          return {key: value for key, value in row.items() if key != 'time'}
    raise AssertionError(('publication deadline', self.scenario, self.output, self.rows[-4:]))

  def request(self, body, delay=0):
    self.server.responses.append({'body': json.dumps(body).encode(), 'header_delay': delay})

  def stop(self, signum):
    start = time.monotonic()
    self.process.send_signal(signum)
    code = self.process.wait(timeout=3)
    return {'returncode': code, 'elapsed': time.monotonic() - start}

  def close(self):
    if self.process.poll() is None:
      self.process.kill()
      self.process.wait(timeout=3)
    self.stdout.close()
    self.stderr.close()
    (self.output / 'publications.json').write_text(json.dumps(self.rows, indent=2) + '\n')
    (self.output / 'requests.json').write_text(json.dumps(self.server.rows, indent=2) + '\n')
    self.server.close()
    self.subscribers.clear()
    self.publisher = None
    self.stack.close()


def instruction(row, valid):
  return row['service'] == 'navInstruction' and row['valid'] == valid


def run(implementation, scenario, args):
  output = args.output / f'{implementation}-{scenario}'
  output.mkdir()
  peer = Peer(implementation, scenario, args, output)
  selected = []
  try:
    selected.append(peer.wait(lambda row: instruction(row, False)))
    identity = {'command': peer.command, 'exe': os.readlink(f'/proc/{peer.process.pid}/exe'),
                'maps': Path(f'/proc/{peer.process.pid}/maps').read_text()}
    if implementation == 'native':
      assert 'libpython' not in identity['maps']
      if args.runner:
        assert str(args.binary.resolve()) in identity['maps'], identity
      else:
        assert identity['exe'] == str(args.binary.resolve())
    (output / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
    if scenario in ('interrupt_http', 'terminate'):
      if scenario == 'interrupt_http':
        peer.request(route(), delay=11)
        peer.put('NavDestination', parameters()['NavDestination'])
        deadline = time.monotonic() + 4
        while not peer.server.rows and time.monotonic() < deadline:
          peer.pump()
        assert peer.server.rows, 'HTTP request did not start'
      stopped = peer.stop(signal.SIGINT if scenario == 'interrupt_http' else signal.SIGTERM)
      assert stopped['returncode'] == (0 if scenario == 'interrupt_http' else -signal.SIGTERM), stopped
      assert stopped['elapsed'] < 2, stopped
    else:
      peer.request(route())
      peer.put('NavDestinationWaypoints', '[[127.0002,37.0]]')
      peer.put('NavDestination', parameters()['NavDestination'])
      first = peer.wait(lambda row: row['service'] == 'navRouteNavd' and bool(row['data']['coordinates']))
      selected.extend([first, peer.wait(lambda row: instruction(row, True))])
      peer.pending.clear()
      selected.append(peer.wait(lambda row: instruction(row, True)))
      peer.pending.clear()
      changed = route()
      changed['routes'][0]['legs'][0]['steps'][0]['geometry']['coordinates'].insert(1, [127.0007, 37.])
      peer.request(changed, delay=6.5 if scenario == 'route_lifecycle' else 0)
      peer.put('NavDestination', json.dumps({'latitude': 37., 'longitude': 127.0024}))
      peer.ui_pid = 20
      started = time.monotonic()
      deadline = started + 4
      while len(peer.server.rows) < 2 and time.monotonic() < deadline:
        peer.pump()
      assert len(peer.server.rows) == 2, 'second HTTP request did not start'
      http_started = time.monotonic()
      if scenario == 'route_lifecycle':
        resent = peer.wait(lambda row: row['service'] == 'navRouteNavd' and row['data'] == first['data'], timeout=6)
        timer_delay = time.monotonic() - http_started
        assert 4.2 < timer_delay < 6.2, ('timer did not publish during blocked HTTP', timer_delay)
        selected.append(resent)
      current = peer.wait(lambda row: row['service'] == 'navRouteNavd' and row['data'] != first['data'])
      selected.append(current)
      if scenario == 'timer_latest_route':
        selected.append(peer.wait(lambda row: row['service'] == 'navRouteNavd' and row['data'] == current['data'], timeout=6))
        assert 4.2 < time.monotonic() - http_started < 6.2, 'latest route resend deadline'
      selected.append(peer.wait(lambda row: instruction(row, True)))
      peer.pending.clear()
      peer.put('NavDestination', None)
      selected.append(peer.wait(lambda row: row['service'] == 'navRouteNavd' and not row['data']['coordinates']))
      selected.append(peer.wait(lambda row: instruction(row, False)))
      if scenario == 'timer_cleared_route':
        selected.append(peer.wait(lambda row: row['service'] == 'navRouteNavd' and not row['data']['coordinates'], timeout=6))
        assert 4.2 < time.monotonic() - http_started < 6.2, 'cleared route resend deadline'
      stopped = peer.stop(signal.SIGINT)
      assert stopped['returncode'] == 0 and stopped['elapsed'] < 2, stopped
    result = {'publications': selected, 'returncode': stopped['returncode'],
              'params': {path.name: path.read_text() for path in peer.params.iterdir() if path.is_file()}}
    (output / 'selected.json').write_text(json.dumps(result, indent=2) + '\n')
    (output / 'stop.json').write_text(json.dumps(stopped, indent=2) + '\n')
    return result
  finally:
    peer.close()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument("--runner", nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  comparisons = []
  for scenario in ('route_lifecycle', 'timer_latest_route', 'timer_cleared_route', 'interrupt_http', 'terminate'):
    source = run('source', scenario, args)
    native = run('native', scenario, args)
    comparisons.append({'scenario': scenario, 'passed': source == native})
  files = [args.binary, args.binding, Path(__file__), ROOT / 'rust/tools/navd_runtime_source.py',
           ROOT / 'openpilot/selfdrive/navd/navd.py', ROOT / 'openpilot/selfdrive/navd/helpers.py', ROOT / 'openpilot/common/realtime.py']
  files.extend((ROOT / 'rust/crates/navd').rglob('*.rs'))
  report = {'status': 'PASS' if all(row['passed'] for row in comparisons) else 'FAIL', 'comparisons': comparisons,
            'files': {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'status': report['status'], 'comparisons': comparisons}))
  raise SystemExit(report['status'] != 'PASS')


if __name__ == '__main__':
  main()

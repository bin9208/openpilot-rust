#!/usr/bin/env python3
"""Fault actual uploader log sockets and compare exception/HTTP/xattr boundaries."""

import argparse
import json
import logging
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace

import zmq
from check_uploader import definitions, endpoint, make_files, signing_key, source
from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def source_run(config):
  saturated = config['target'] == 'eagain'
  log_endpoint = config['endpoint'] + '-saturated' if saturated else config['endpoint']
  module, swaglog = load(Path(config['binding']), log_endpoint, Path(config['root']).parent / 'source-logs')
  params = module.Params(str(Path(config['root']).parent / 'params'))
  healthy = swaglog.ipchandler
  closed = swaglog.UnixDomainSocketHandler(swaglog.SwagFormatter(swaglog.cloudlog))
  closed.connect()
  closed.sock.close()
  if saturated:
    healthy.connect()
    for _ in range(10000):
      try:
        healthy.sock.send(b'queue fixture', zmq.NOBLOCK)
      except zmq.Again:
        break
    else:
      raise AssertionError('ZMQ queue did not reach EAGAIN')
  attempts, fired = [], False

  class FaultHandler(logging.Handler):
    def emit(self, record):
      nonlocal fired
      value = json.loads(healthy.format(record))['msg']
      label = value['event'] if isinstance(value, dict) else ('debug_url' if value.startswith('upload_url v1.4') else value)
      attempts.append(label)
      if (not fired and label == config['target']) or (fired and config['persistent']):
        fired = True
        closed.emit(record)
      else:
        healthy.emit(record)

  swaglog.cloudlog.removeHandler(healthy)
  swaglog.cloudlog.addHandler(FaultHandler())
  uploader, _ = source(ROOT, {'root': config['root'], 'fail_mark': config['fail_mark']}, {'api_host': config['api'], 'persist': config['persist']})
  uploader.params = params
  namespace = uploader.upload.__globals__
  namespace['cloudlog'] = swaglog.cloudlog
  error = None
  try:
    if config['action'] == 'clear':
      namespace['clear_locks'](config['root'])
      result = None
    elif config['action'] == 'upload':
      result = uploader.upload('qlog', 'route--0/qlog.zst', str(Path(config['root']) / 'route--0/qlog'), 1, False)
    elif config['action'] == 'direct_step':
      result = uploader.step(1, False)
    else:
      results, delays = [], []
      step = uploader.step

      def track_step(network_type, metered):
        result = step(network_type, metered)
        results.append(result)
        return result

      uploader.step = track_step

      class Stop:
        calls = 0

        def is_set(self):
          self.calls += 1
          return self.calls > 1

      class Subscriber:
        def update(self, timeout):
          pass

        def __getitem__(self, key):
          return SimpleNamespace(networkType=SimpleNamespace(raw=1), networkMetered=False)

      namespace.update(
        Uploader=lambda identity, root: uploader,
        Params=lambda: params,
        Paths=SimpleNamespace(log_root=lambda: config['root']),
        set_core_affinity=lambda cpus: None,
        messaging=SimpleNamespace(SubMaster=lambda services: Subscriber()),
        NetworkType=SimpleNamespace(none=0, wifi=1),
        threading=threading,
        force_wifi=True,
        allow_sleep=True,
        random=SimpleNamespace(uniform=lambda lo, hi: 0),
        time=SimpleNamespace(monotonic=time.monotonic, sleep=delays.append),
      )
      params.put('DongleId', '0000000000000000')
      exec(definitions(ROOT / 'openpilot/system/loggerd/uploader.py', {'main'}), namespace)
      namespace['main'](Stop())
      assert len(results) == len(delays) == 1
      result = results[0]
    status = 'Idle' if result is None else 'Success' if result else 'Failure'
    delay = 5.0 if result is None else 0.1 if result else 0.2
    if config['action'] == 'step':
      assert delays == [delay], delays
  except Exception as exception:
    status, error, delay = 'Error', repr(exception), None
  finally:
    healthy.close()
    closed.close()
  print(
    json.dumps({'status': status, 'error': error, 'delay': delay, 'last': uploader.last_filename, 'attempts': attempts, 'fired': fired, 'saturated': saturated})
  )
  raise SystemExit(int(status == 'Error'))


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source', type=Path)
  parser.add_argument('--native', type=Path)
  parser.add_argument('--collector', type=Path)
  parser.add_argument('--binding', type=Path)
  parser.add_argument('--output', type=Path)
  parser.add_argument('--expect-mismatch', action='store_true')
  args = parser.parse_args()
  if args.source:
    source_run(json.loads(args.source.read_text()))
    return
  from check_uploader_logging import disk_check, drain, normalized
  from logmessaged_native import Peer

  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  cases = [
    ('normal', '', {}),
    ('eagain', 'eagain', {}),
    ('persistent-debug', 'debug_url', {'persistent': True}),
    ('backoff', 'upload backoff 0.1', {'put_status': 500}),
    ('start', 'upload_start', {}),
    ('debug', 'debug_url', {}),
    ('success', 'upload_success', {}),
    ('ignored', 'upload_ignored', {'api_status': 412}),
    ('failed', 'upload_failed', {'put_status': 500}),
    ('large', 'uploader_too_large', {'size': 25_000_001}),
    ('metadata', 'uploader_getxattr_failed', {'broken': True}),
    ('listdir', 'listdir_by_creation failed', {'no_listing': True, 'action': 'direct_step'}),
    ('getsize', 'upload: getsize failed', {'missing': True, 'action': 'upload'}),
    ('clear', 'clear_locks failed', {'clear_error': True, 'action': 'clear'}),
    ('mark', 'uploader_setxattr_failed', {'fail_mark': True}),
  ]
  rows, differences = [], []
  for name, target, settings in cases:
    for original_collector in [True, False]:
      expected = None
      for original in [True, False]:
        output = args.output / name / (('source' if original else 'native') + ('-original-collector' if original_collector else '-rust-collector'))
        peer = Peer(args.collector.resolve(), output, original_collector)
        requests = []
        try:
          peer.start()
          with tempfile.TemporaryDirectory(prefix='uploader-log-fault-') as temporary, endpoint(settings, requests) as api:
            root = Path(temporary)
            logs = root / 'logs'
            logs.mkdir()
            path = logs / 'route--0/qlog'
            if not settings.get('missing'):
              make_files(logs, [('route--0/qlog', settings.get('size', 128), False)])
            if settings.get('broken'):
              path.unlink()
              path.symlink_to('missing')
            if settings.get('clear_error'):
              (logs / 'plain').write_text('fixture')
            if settings.get('no_listing'):
              logs.chmod(0o111)
            persist = root / 'persist'
            signing_key(persist, 'id_ecdsa')
            config = {
              'root': str(logs),
              'endpoint': peer.endpoint,
              'target': target,
              'api': api,
              'persist': str(persist),
              'version': str(ROOT / 'openpilot/common/version.h'),
              'binding': str(args.binding.resolve()),
              'action': settings.get('action', 'step'),
              'fail_mark': settings.get('fail_mark', False),
              'persistent': settings.get('persistent', False),
            }
            config_path = output / 'input.json'
            config_path.write_text(json.dumps(config, indent=2) + '\n')
            command = [sys.executable, str(Path(__file__).resolve()), '--source', str(config_path)] if original else [str(args.native.resolve())]
            result = subprocess.run(command, input=None if original else json.dumps(config).encode(), capture_output=True, timeout=30)
            (output / 'stdout.log').write_bytes(result.stdout)
            (output / 'stderr.log').write_bytes(result.stderr)
            observed = json.loads(result.stdout)
            assert result.returncode == int(observed['status'] == 'Error'), result.stderr
            logs.chmod(0o755)
            try:
              marked = os.getxattr(path, 'user.upload') == b'1'
            except OSError:
              marked = False
            records = drain(peer, output)
            code, _ = peer.stop()
            assert code in (0, -signal.SIGINT) if original_collector else code == 0
            disks = disk_check(peer, records)
            if name == 'debug':
              failure = next(
                json.loads(raw[1:])['msg']
                for raw in records
                if isinstance(json.loads(raw[1:])['msg'], dict) and json.loads(raw[1:])['msg'].get('event') == 'upload_failed'
              )
              assert failure['stat'] is None and 'Socket operation on non-socket' in failure['exc'][0]
              assert failure['exc'][0].startswith('ZMQError(' if original else 'Logging(Transport(')
            if observed['error'] and not original:
              assert 'Logging(Transport(Socket operation on non-socket))' in observed['error']
            comparable = normalized(records, logs)
            for record in comparable:
              if isinstance(record['msg'], str) and record['msg'].startswith('upload_url v1.4 '):
                assert api + '/put' in record['msg']
                record['msg'] = record['msg'].replace(api, '<local-api>')
            observed['last'] = str(Path(observed['last']).relative_to(logs)) if observed['last'] else ''
            comparison = {key: observed[key] for key in ['status', 'delay', 'last', 'attempts', 'fired', 'saturated']}
            comparison.update(marked=marked, requests=[request[0] for request in requests], records=comparable)
            if original:
              expected = comparison
              expected_status = 'Success' if name in ['normal', 'eagain'] else 'Failure' if name == 'debug' else 'Error'
              assert observed['status'] == expected_status, (name, observed)
              assert observed['fired'] == bool(target and target != 'eagain')
              if observed['error']:
                assert 'ZMQError' in observed['error']
            elif comparison != expected:
              differences.append({'case': name, 'collector': original_collector, 'source': expected, 'native': comparison})
            row = {
              'name': name,
              'original': original,
              'original_collector': original_collector,
              'argv': command,
              'exit_code': result.returncode,
              'observation': observed,
              'marked': marked,
              'http': [{'method': method, 'path': path, 'headers': headers, 'body_hex': body.hex()} for method, path, headers, body in requests],
              'disk_records': disks,
              'records': [json.loads(raw[1:]) for raw in records],
            }
            (output / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
            rows.append(row)
        finally:
          peer.close()
  report = {'passed': not differences, 'scenarios': rows, 'differences': differences}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': not differences, 'scenarios': len(rows), 'differences': len(differences)}))
  if bool(differences) != args.expect_mismatch:
    raise SystemExit(1)


if __name__ == '__main__':
  main()

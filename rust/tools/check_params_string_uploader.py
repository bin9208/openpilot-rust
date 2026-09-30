#!/usr/bin/env python3
"""Exercise uploader's typed getter against Cython and both actual log collectors."""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
from types import SimpleNamespace

from check_uploader import definitions, endpoint, make_files, signing_key, source
from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def source_main(config):
  module, swaglog = load(Path(config['binding']), 'ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX'], Path(config['output']) / 'source-logs')
  params = module.Params()
  uploader, _ = source(ROOT, {'root': config['root']}, config['http'])
  uploader.params = params
  namespace = uploader.upload.__globals__

  class Stop:
    calls = 0

    def is_set(self):
      self.calls += 1
      return self.calls > 1

  class Subscriber:
    def __getitem__(self, key):
      return SimpleNamespace(networkType=SimpleNamespace(raw=0), networkMetered=False)

    def update(self, timeout):
      pass

  namespace.update(
    cloudlog=swaglog.cloudlog,
    Uploader=lambda identity, root: uploader,
    Params=module.Params,
    Paths=SimpleNamespace(log_root=lambda: config['root']),
    set_core_affinity=lambda cpus: None,
    messaging=SimpleNamespace(SubMaster=lambda services: Subscriber()),
    NetworkType=SimpleNamespace(none=0, wifi=1),
    threading=threading,
    force_wifi=True,
    allow_sleep=False,
  )
  exec(definitions(ROOT / 'openpilot/system/loggerd/uploader.py', {'main'}), namespace)
  try:
    namespace['main'](Stop())
  finally:
    swaglog.ipchandler.close()


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source', type=Path)
  parser.add_argument('--binding', type=Path)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--collector', type=Path)
  parser.add_argument('--output', type=Path)
  args = parser.parse_args()
  if args.source:
    source_main(json.loads(args.source.read_text()))
    return
  import msgq
  from openpilot.cereal.services import SERVICE_LIST
  from check_uploader_logging import disk_check, drain, normalized
  from logmessaged_native import Peer

  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = []
  cases = [
    ('missing-id', None, None),
    ('empty-id', b'', None),
    ('invalid-id', b'one\'\xff', None),
    ('valid-id', b'0000000000000000', None),
    ('empty-routes', b'0000000000000000', b''),
    ('valid-routes', b'0000000000000000', b'dongle|route--0'),
    ('invalid-routes', b'0000000000000000', b'\xff\'"\n'),
  ]
  for name, identity, routes in cases:
    normalized_source = None
    for original_collector in [True, False]:
      for original in [True, False]:
        destination = args.output / name / (('original' if original_collector else 'rust') + '-collector-' + ('source' if original else 'native'))
        peer = Peer(args.collector.resolve(), destination, original_collector)
        publisher = None
        requests = []
        try:
          peer.start()
          publisher = msgq.pub_sock('deviceState', SERVICE_LIST['deviceState'].queue_size)
          with tempfile.TemporaryDirectory(prefix='params-uploader-') as temporary, endpoint({}, requests) as api:
            root = Path(temporary)
            logs = root / 'logs'
            make_files(logs, [('route--0/qlog', 128, False)])
            params = root / 'params' / peer.prefix
            params.mkdir(parents=True)
            for key, data in [('DongleId', identity), ('AthenadRecentlyViewedRoutes', routes), ('IsOffroad', b'0')]:
              if data is not None:
                (params / key).write_bytes(data)
            persist = destination / 'home' / ('.comma' + peer.prefix) / 'persist'
            signing_key(persist, 'id_ecdsa')
            environment = dict(
              os.environ,
              HOME=str(destination / 'home'),
              OPENPILOT_PREFIX=peer.prefix,
              PARAMS_ROOT=str(params.parent),
              LOG_ROOT=str(logs),
              API_HOST=api,
              UPLOADER_SLEEP='0',
              FORCEWIFI='',
            )
            environment.pop('FAKEUPLOAD', None)
            environment.pop('LOGPRINT', None)
            config = {'root': str(logs), 'binding': str(args.binding.resolve()), 'output': str(destination), 'http': {'api_host': api, 'persist': str(persist)}}
            config_path = destination / 'source-config.json'
            config_path.write_text(json.dumps(config))
            command = (
              [sys.executable, str(Path(__file__).resolve()), '--source', str(config_path)] if original else [str(args.binary.resolve()), '--cycles', '1']
            )
            result = subprocess.run(command, env=environment, capture_output=True, timeout=30)
            (destination / 'stdout.log').write_bytes(result.stdout)
            (destination / 'stderr.log').write_bytes(result.stderr)
            expected_code = 1 if name.endswith('-id') and name != 'valid-id' else 0
            assert result.returncode == expected_code, (name, command, result.stderr)
            records = drain(peer, destination)
            exit_code, _ = peer.stop()
            assert exit_code in (0, -signal.SIGINT) if original_collector else exit_code == 0
            count = disk_check(peer, records)
            assert not peer.records['errorLogMessage']
            warnings = [json.loads(raw[1:]) for raw in records if raw[0] == 30]
            assert len(warnings) == int(name.startswith('invalid-')), (name, warnings)
            if warnings and not original:
              assert warnings[0]['funcName'] == 'openpilot_params_typed::get_string' and warnings[0]['filename'] == 'lib.rs'
            comparable = normalized(records, logs)
            for record in comparable:
              if isinstance(record['msg'], str) and record['msg'].startswith('upload_url v1.4 '):
                assert api + '/put' in record['msg']
                record['msg'] = record['msg'].replace(api, '<local-api>')
            if normalized_source is None:
              normalized_source = comparable
            assert comparable == normalized_source, (name, comparable, normalized_source)
            assert len(requests) == (0 if expected_code else 2), (name, requests)
            if expected_code == 0:
              assert os.getxattr(logs / 'route--0/qlog', 'user.upload') == b'1'
            if warnings:
              assert comparable[0]['level'] == 'WARNING'
              if expected_code:
                assert comparable[1]['msg'] == 'uploader missing dongle_id'
            row = {
              'case': name,
              'source': original,
              'original_collector': original_collector,
              'argv': command,
              'exit': result.returncode,
              'warnings': len(warnings),
              'disk_records': count,
              'http_requests': len(requests),
              'records': [json.loads(raw[1:]) for raw in records],
            }
            (destination / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
            rows.append(row)
        finally:
          del publisher
          peer.close()
  (args.output / 'report.json').write_text(json.dumps({'passed': True, 'scenarios': rows}, indent=2) + '\n')
  print(json.dumps({'passed': True, 'scenarios': len(rows)}))


if __name__ == '__main__':
  main()

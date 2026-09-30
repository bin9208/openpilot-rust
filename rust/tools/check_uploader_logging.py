#!/usr/bin/env python3
"""Compare actual uploader log records, collector publications, and disk formatting."""

import argparse
import json
import logging
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import threading
import time
import traceback
from types import SimpleNamespace

import msgq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from openpilot.common.logging_extra import SwagFormatter, SwagLogFileFormatter, SwagLogger
from check_uploader import definitions, endpoint, make_files, observe, signing_key, source
from logmessaged_native import Peer

ROOT = Path(__file__).resolve().parents[2]


def original_records(directory, http, settings, direct=None):
  records = []
  logger = SwagLogger()
  logger.setLevel(logging.DEBUG)
  formatter = SwagFormatter(logger)

  class Capture(logging.Handler):
    def emit(self, record):
      records.append(bytes([record.levelno]) + formatter.format(record).encode())

  logger.addHandler(Capture())
  uploader, _ = source(ROOT, {'root': str(directory)}, http)
  namespace = uploader.upload.__globals__
  namespace['cloudlog'] = logger
  if direct is not None:
    observe(uploader, [], {'root': str(directory), **direct})
    return records

  class Stop:
    count = 0

    def is_set(self):
      self.count += 1
      return self.count > 1

  class Subscriber:
    def __getitem__(self, key):
      return SimpleNamespace(networkType=SimpleNamespace(raw=0), networkMetered=False)

    def update(self, timeout):
      pass

  namespace.update(
    Uploader=lambda identity, root: uploader,
    Params=lambda: SimpleNamespace(get=lambda key: None if settings.get('missing_id') else '0000000000000000', get_bool=lambda key: False),
    Paths=SimpleNamespace(log_root=lambda: str(directory)),
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
  except Exception as error:
    assert settings.get('missing_id') and str(error) == "uploader can't start without dongle id", error
  return records


def drain(peer, output):
  records = []
  last = time.monotonic()
  while time.monotonic() - last < 0.25:
    received = False
    for service, subscriber in peer.subscribers.items():
      packet = subscriber.receive(non_blocking=True)
      if packet is None:
        continue
      received = True
      last = time.monotonic()
      with log.Event.from_bytes(packet) as event:
        assert event.valid and event.which() == service
        raw = getattr(event, service)
        value = json.loads(raw)
      with (output / (service + '.bin')).open('ab') as stream:
        stream.write(packet)
      peer.records[service].append(value)
      if service == 'logMessage':
        records.append(bytes([value['levelnum']]) + raw.encode())
    if not received:
      time.sleep(0.005)
  return records


def disk_check(peer, records):
  actual = []
  for path in sorted(peer.root.glob('swaglog.*')):
    actual.extend(json.loads(line) for line in path.read_text().splitlines())
  formatter = SwagLogFileFormatter(None)
  expected = [json.loads(formatter.format(raw[1:].decode())) for raw in records if raw[0] >= logging.INFO]
  for value in actual + expected:
    value.pop('id')
  assert actual == expected, ('disk', actual, expected)
  return len(actual)


def native_records(binary, collector, output, directory, http, settings, original_collector, direct=None):
  peer = Peer(collector, output, original_collector)
  publisher = None
  try:
    peer.start()
    publisher = msgq.pub_sock('deviceState', SERVICE_LIST['deviceState'].queue_size)
    params = directory.parent / ('params-' + output.name) / peer.prefix
    params.mkdir(parents=True)
    if not settings.get('missing_id'):
      (params / 'DongleId').write_text('0000000000000000')
    (params / 'IsOffroad').write_text('0')
    persist = output / 'home' / ('.comma' + peer.prefix) / 'persist'
    signing_key(persist, 'id_ecdsa')
    environment = dict(
      os.environ,
      HOME=str(output / 'home'),
      OPENPILOT_PREFIX=peer.prefix,
      PARAMS_ROOT=str(params.parent),
      LOG_ROOT=str(directory),
      API_HOST=http['api_host'],
      UPLOADER_SLEEP='0',
      FORCEWIFI='',
    )
    environment.pop('FAKEUPLOAD', None)
    environment.pop('LOGPRINT', None)
    if settings.get('fake'):
      environment['FAKEUPLOAD'] = ''
    command = [str(binary), '--cycles', '1'] if direct is None else [str(binary)]
    input_bytes = None if direct is None else (json.dumps({'root': str(directory), 'log_endpoint': peer.endpoint, **direct}) + '\n').encode()
    run = subprocess.run(command, input=input_bytes, env=environment, capture_output=True, timeout=20)
    (output / 'uploader.stdout').write_bytes(run.stdout)
    (output / 'uploader.stderr').write_bytes(run.stderr)
    assert run.returncode == (1 if settings.get('missing_id') else 0), run.stderr
    records = drain(peer, output)
    code, _ = peer.stop()
    assert code in (0, -signal.SIGINT) if original_collector else code == 0
    disks = disk_check(peer, records)
    assert len(peer.records['errorLogMessage']) == sum(raw[0] >= logging.ERROR for raw in records)
    return records, {'exit_code': run.returncode, 'disk_records': disks, 'error_records': len(peer.records['errorLogMessage']), 'stderr': run.stderr.decode()}
  finally:
    del publisher
    peer.close()


def normalized(records, directory):
  output = []
  for raw in records:
    value = json.loads(raw[1:])
    assert value['name'] == 'swaglog' and value['levelnum'] == raw[0]
    message = value['msg']
    if isinstance(message, dict):
      message = dict(message)
      if 'fn' in message:
        message['fn'] = str(Path(message['fn']).relative_to(directory))
      if 'speed' in message:
        assert message['speed'] >= 0
        message['speed'] = '<measured>'
      if message.get('exc') is not None:
        assert isinstance(message['exc'], list) and len(message['exc']) == 2, message
        assert all(isinstance(item, str) and item for item in message['exc'])
        message['exc'] = ['<native or Python error representation>', '<native or Python trace>']
    result = {'level': value['level'], 'levelnum': raw[0], 'msg': message}
    if 'exc_info' in value:
      assert isinstance(value['exc_info'], str) and value['exc_info']
      result['exc_info'] = '<native or Python trace>'
    output.append(result)
  return output


def validate_origin(records, native):
  locations = []
  for raw in records:
    record = json.loads(raw[1:])
    assert record['host'] == os.uname().nodename and record['process'] > 0 and record['thread'] > 0
    assert record['created'] > 0 and record['threadName']
    if not native:
      if isinstance(record['msg'], str) and (record['msg'].startswith('upload backoff ') or record['msg'] == 'uploader missing dongle_id'):
        # The unchanged SwagLogger.findCaller reports the source main's caller for this direct INFO call.
        assert record['pathname'] == str(Path(__file__).resolve()) and record['funcName'] == 'original_records'
      else:
        assert record['pathname'] == str(ROOT / 'openpilot/system/loggerd/uploader.py')
      continue
    assert record['ctx']['runtime_language'] == 'rust' and len(record['ctx']['source_commit']) == 40
    path = ROOT / 'rust' / record['pathname']
    assert path.is_file() and 'log_site!' in path.read_text().splitlines()[record['lineno'] - 1]
    message = record['msg']
    if isinstance(message, dict):
      label = message['event']
      expected = ('scan.rs', 'list_upload_files') if label == 'uploader_getxattr_failed' else ('lib.rs', 'upload')
      if message.get('exc') is not None:
        assert isinstance(message['exc'], list) and len(message['exc']) == 2
        assert all(isinstance(item, str) and item for item in message['exc'])
        assert message['exc'][1].startswith('Rust error:') and 'Rust backtrace captured at the reporting site:' in message['exc'][1]
        assert 'Traceback (most recent call last)' not in message['exc'][1]
    elif message.startswith('upload_url v1.4 '):
      expected = ('http.rs', 'do_upload')
    elif message in ['clear_locks failed', 'listdir_by_creation failed']:
      expected = ('scan.rs', 'clear_locks' if message.startswith('clear_locks') else 'list_upload_files')
    elif message == 'upload: getsize failed':
      expected = ('lib.rs', 'upload')
    else:
      expected = ('runtime.rs', 'run')
    assert record['filename'] == expected[0] and record['funcName'].endswith('::' + expected[1]), record
    if 'exc_info' in record:
      assert record['level'] == 'ERROR' and record['exc_info'].startswith('Rust error:')
      assert 'Rust backtrace captured at the reporting site:' in record['exc_info']
      assert 'Traceback (most recent call last)' not in record['exc_info']
    locations.append({key: record[key] for key in ['pathname', 'lineno', 'funcName', 'level']})
  return locations


def validate_error_meaning(name, records, directory, native):
  values = [json.loads(raw[1:]) for raw in records]
  if name == 'invalid-json':
    failure = next(value['msg'] for value in values if isinstance(value['msg'], dict) and value['msg'].get('event') == 'upload_failed')
    assert failure['stat'] is None and isinstance(failure['exc'], list)
    assert failure['exc'][0].startswith('Json(' if native else 'JSONDecodeError(')
  elif name in ['getsize-error', 'listdir-error', 'clear-locks-error']:
    categories = {
      'getsize-error': ('FileNotFoundError', 'NotFound'),
      'listdir-error': ('PermissionError', 'PermissionDenied'),
      'clear-locks-error': ('NotADirectoryError', 'NotADirectory'),
    }
    errors = [value['exc_info'] for value in values if 'exc_info' in value]
    assert errors and all(categories[name][int(native)] in error and str(directory) in error for error in errors)


def fixture(directory, settings):
  if settings.get('broken'):
    (directory / 'route--0').mkdir(parents=True)
    (directory / 'route--0/qlog').symlink_to('absent')
  elif settings.get('empty_root'):
    directory.mkdir()
  else:
    make_files(directory, [('route--0/qlog', settings.get('size', 128), False)])
    if settings.get('readonly'):
      (directory / 'route--0/qlog').chmod(0o444)
  if settings.get('clear_error'):
    (directory / 'plain').write_text('fixture')
  if settings.get('no_listing'):
    directory.chmod(0o111)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--collector', type=Path, required=True)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.binary = args.binary.resolve()
  args.collector = args.collector.resolve()
  args.trace = args.trace.resolve()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  rows, failures = [], []
  cases = [
    ('missing-id', {'missing_id': True, 'empty_root': True}),
    ('status-500', {'put_status': 500}),
    ('invalid-json', {'bad_json': True}),
    ('success', {}),
    ('ignored', {'api_status': 412}),
    ('fake', {'fake': True}),
    ('clear-locks-error', {'clear_error': True}),
    ('unauthorized-accepted', {'put_status': 401}),
    ('forbidden-accepted', {'put_status': 403}),
    ('created', {'put_status': 201}),
    ('put-ignored', {'put_status': 412}),
    ('zero-size', {'size': 0}),
    ('oversized', {'size': 25_000_001}),
    ('mark-error', {'readonly': True}),
    ('metadata-error', {'broken': True}),
    (
      'header-repr',
      {'fake': True, 'headers': {'z': "one's value", 'a': 'both\'" quotes', '한글': '😀\n\t\\', 'nested': [True, None, 1e-7, -0.0, {'z': 2, 'a': 1}]}},
    ),
    ('getsize-error', {'empty_root': True, 'direct': {'upload': 'route--0/qlog'}}),
    ('listdir-error', {'empty_root': True, 'no_listing': True, 'direct': {'steps': 1}}),
  ]
  for name, settings in cases:
    case = args.output / name
    case.mkdir()
    with tempfile.TemporaryDirectory(prefix='uploader-logging-') as temporary, endpoint(settings, []) as host:
      temporary = Path(temporary)
      persist = temporary / 'persist'
      signing_key(persist, 'id_ecdsa')
      http = {'api_host': host, 'persist': str(persist), 'fake': settings.get('fake', False)}
      original = temporary / 'original'
      fixture(original, settings)
      expected = original_records(original, http if settings.get('direct') is None else None, settings, settings.get('direct'))
      original.chmod(0o755)
      (case / 'source.json').write_text(json.dumps([json.loads(raw[1:]) for raw in expected], indent=2) + '\n')
      for original_collector in [True, False]:
        destination = case / ('original-collector' if original_collector else 'rust-collector')
        directory = temporary / destination.name
        fixture(directory, settings)
        direct = settings.get('direct')
        records, result = native_records(
          args.binary if direct is None else args.trace, args.collector, destination, directory, http, settings, original_collector, direct
        )
        directory.chmod(0o755)
        (destination / 'records.json').write_text(json.dumps([json.loads(raw[1:]) for raw in records], indent=2) + '\n')
        try:
          validate_origin(expected, False)
          validate_error_meaning(name, expected, original, False)
          validate_error_meaning(name, records, directory, True)
          assert normalized(expected, original) == normalized(records, directory), 'source/native record fields or levels differ'
          result['callsites'] = validate_origin(records, True)
          assert 'uploader: ready' not in result['stderr'], 'invented readiness diagnostic'
          if result['exit_code'] == 0 and not any(raw[0] >= logging.ERROR for raw in records):
            assert not result['stderr'], 'unexpected console output below the original warning threshold'
        except AssertionError as error:
          failures.append({'case': name, 'collector': destination.name, 'error': str(error), 'trace': traceback.format_exc()})
        rows.append({'case': name, 'collector': destination.name, **result})
  report = {'passed': not failures, 'scenarios': rows, 'failures': failures}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': report['passed'], 'scenarios': len(rows), 'failures': failures}, indent=2))
  if failures:
    raise SystemExit(1)


if __name__ == '__main__':
  main()

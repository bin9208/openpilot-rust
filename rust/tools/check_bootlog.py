"""Compare actual C++ and Rust boot artifacts with controlled external inputs."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import random
import resource
import subprocess
import time

import zstandard
from openpilot.cereal import log
from check_uploader import source as uploader_source

ROOT = Path(__file__).resolve().parents[2]


def encoded(value):
  if isinstance(value, bytes):
    return {'hex': value.hex()}
  if isinstance(value, dict):
    return {key: encoded(item) for key, item in value.items()}
  if isinstance(value, list):
    return [encoded(item) for item in value]
  return value


def run(binary, output, config, native, runner):
  output.mkdir(parents=True)
  prefix = 'boot-fixture-' + hashlib.sha256(str(output).encode()).hexdigest()[:12]
  params = output / 'params' / prefix
  params.mkdir(parents=True)
  files = {'BootCount': config.get('count', b'42'), 'DongleId': b'boot-fixture', 'GitCommit': b'1' * 40, 'AccessToken': b'secret-synthetic'}
  for key, value in files.items():
    (params / key).write_bytes(value)
  if config.get('counter_directory'):
    (params / 'BootCount').unlink()
    (params / 'BootCount').mkdir()
  if config.get('current_directory'):
    (params / 'CurrentBootlog').mkdir()
  copy = output / 'copy' / prefix
  copy.mkdir(parents=True)
  for key, value in files.items():
    (copy / key).write_bytes(b'snapshot-fixture' if key == 'DongleId' else value)
  pstore = output / 'pstore'
  if not config.get('missing_pstore'):
    pstore.mkdir()
    (pstore / 'z-crash').write_bytes(random.Random(7).randbytes(400_000) if config.get('large') else b'crash\x00\xff\n')
    (pstore / 'a-crash').write_bytes(b'first\n')
    (pstore / 'directory').mkdir()
    (pstore / 'linked-dir').symlink_to('directory')
    (pstore / 'broken').symlink_to('absent')
    (pstore / 'linked-file').symlink_to('a-crash')
  launch = output / 'launch_log'
  if not config.get('missing_launch'):
    launch.write_bytes(config.get('launch', b'launch\x00log\n'))
  commands = output / 'commands'
  commands.mkdir()
  (commands / 'df').write_text('#!/bin/sh\nprintf "synthetic df\\n"\n')
  (commands / 'df').chmod(0o755)
  if not config.get('missing_journal'):
    journal_bytes = output / 'journal.stdout'
    journal_bytes.write_bytes(config.get('journal', b'journal fixture\n'))
    (commands / 'journalctl').write_text(f'#!/bin/sh\n/bin/cat "{journal_bytes}"\nexit {config.get("journal_exit", 0)}\n')
    (commands / 'journalctl').chmod(0o644 if config.get('journal_nonexec') else 0o755)
  cwd = output / 'basedir/openpilot/system/loggerd'
  cwd.mkdir(parents=True)
  (cwd / '../../git_src_commit').write_text('source-commit\n')
  (cwd / '../../git_src_commit_date').write_text('source-date\n')
  logs = output / 'logs'
  if config.get('logs_file'):
    logs.write_text('not directory')
  environment = dict(
    os.environ,
    PARAMS_ROOT=str(params.parent),
    OPENPILOT_PREFIX=prefix,
    LOG_ROOT=str(logs),
    PARAMS_COPY_PATH=str(copy.parent),
    PATH=str(commands),
    BOOTLOG_PSTORE_PATH=str(pstore),
    BOOTLOG_LAUNCH_PATH=str(launch),
    CLEAN='1',
  )
  if config.get('output_fault'):
    environment['LD_PRELOAD'] = str(config['fault_library'])
    environment['BOOTLOG_OUTPUT_FAULT'] = config['output_fault']
  command = [*runner, str(binary)] if native else [str(binary)]
  if native:
    command += ['--pstore', str(pstore), '--launch-log', str(launch)]
  started = time.time_ns()
  process = subprocess.run(command, cwd=cwd, env=environment, capture_output=True, preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_CORE, (0, 0)))
  ended = time.time_ns()
  (output / 'stderr.log').write_bytes(process.stderr or b'<empty>\n')
  (output / 'invocation.json').write_text(
    json.dumps(
      {
        'argv': command,
        'cwd': str(cwd),
        'environment': {
          key: environment[key]
          for key in [
            'PARAMS_ROOT',
            'OPENPILOT_PREFIX',
            'LOG_ROOT',
            'PARAMS_COPY_PATH',
            'PATH',
            'BOOTLOG_PSTORE_PATH',
            'BOOTLOG_LAUNCH_PATH',
            'CLEAN',
            *(['LD_PRELOAD', 'BOOTLOG_OUTPUT_FAULT'] if config.get('output_fault') else []),
          ]
        },
        'exit': process.returncode,
        'start_wall_ns': started,
        'end_wall_ns': ended,
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
      },
      indent=2,
    )
  )
  result = {
    'success': process.returncode == 0,
    'counter': '<directory>' if (params / 'BootCount').is_dir() else (params / 'BootCount').read_bytes().hex(),
    'current_exists': (params / 'CurrentBootlog').exists(),
  }
  if config.get('output_fault'):
    expected_success = config['output_fault'] == 'full' and not config.get('large')
    expected_current = config['output_fault'] == 'close' or expected_success
    assert result['success'] is expected_success and result['current_exists'] is expected_current, result
  if (result['success'] and not config.get('output_fault')) or config.get('output_fault') == 'close':
    candidates = list((logs / 'boot').glob('*.zst'))
    assert len(candidates) == 1
    current = candidates[0].stem
    if (params / 'CurrentBootlog').is_file():
      assert (params / 'CurrentBootlog').read_text() == current
    assert re.fullmatch(r'[0-9a-f]{8}--[0-9a-f]{10}', current), current
    artifact = logs / f'boot/{current}.zst'
    assert artifact.is_file()
    with zstandard.ZstdDecompressor().stream_reader(io.BytesIO(artifact.read_bytes())) as stream:
      raw = stream.read()
    (output / 'artifact.capnp').write_bytes(raw)
    events = [message.to_dict() for message in log.Event.read_multiple_bytes(raw)]
    assert [set(event).intersection({'initData', 'boot'}).pop() for event in events] == ['initData', 'boot']
    (output / 'decoded.json').write_text(json.dumps(encoded(events), indent=2))
    for event in events:
      assert event['valid'] is True and event.pop('logMonoTime') > 0
      body = event.get('initData', event.get('boot'))
      assert started <= body.pop('wallTimeNanos') <= ended
    init = events[0]['initData']
    init['commands']['entries'] = [entry for entry in init['commands']['entries'] if not entry['key'].startswith('loggerd ')]
    uploader, _ = uploader_source(ROOT, {'root': str(logs)})
    selected = uploader.next_file_to_upload(False)
    assert selected == (artifact.name, f'boot/{artifact.name}', str(artifact)), selected
    result.update(identifier_prefix=current[:8], events=encoded(events), upload_key=f'boot/{current[:8]}--RANDOM.zst')
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('original', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--fault-library', type=Path)
  parser.add_argument('--case')
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output = args.output.resolve()
  cases = [
    ('ordinary', {}),
    ('missing_pstore', {'missing_pstore': True}),
    ('missing_launch', {'missing_launch': True}),
    ('missing_journal', {'missing_journal': True}),
    ('nonexec_journal', {'journal_nonexec': True}),
    ('nonzero_journal', {'journal_exit': 7}),
    ('nul_journal', {'journal': b'a\x00b\nc\x00d' + b'q' * 140 + b'\nlast'}),
    ('logs_not_directory', {'logs_file': True}),
    ('counter_directory', {'counter_directory': True}),
    ('current_directory', {'current_directory': True}),
  ]
  for value in [b'', b'junk', b' -1suffix', b'4294967295', b'18446744073709551616', b'  +55rest']:
    cases.append(('counter_' + value.hex(), {'count': value}))
  if args.fault_library:
    for name, fault, large in [
      ('open_failure', 'open', False),
      ('small_write_failure', 'full', False),
      ('large_write_failure', 'full', True),
      ('close_failure', 'close', False),
    ]:
      cases.append((name, {'output_fault': fault, 'large': large, 'fault_library': str(args.fault_library.resolve())}))
  results = []
  for name, config in cases:
    if args.case and name != args.case:
      continue
    expected = run(args.original.resolve(), args.output / name / 'source', config, False, [])
    actual = run(args.binary.resolve(), args.output / name / 'native', config, True, args.runner)
    assert actual == expected, (name, expected, actual)
    results.append({'scenario': name, 'result': 'PASS'})
    print(name, 'PASS', flush=True)
  (args.output / 'result.json').write_text(json.dumps({'result': 'PASS', 'cases': results}, indent=2))


if __name__ == '__main__':
  main()

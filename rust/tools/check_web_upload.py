# /// script
# requires-python = ">=3.12"
# dependencies = ["aiohttp==3.13.3", "anyio==4.12.1", "requests==2.34.2"]
# ///
# Run: uv run rust/tools/check_web_upload.py --binary rust/target/debug/examples/web_upload_trace --output /tmp/web-upload-evidence
"""Execute original helpers and Rust against the same isolated HTTP fixtures."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import tempfile
import threading
from functools import partial
from pathlib import Path

import aiohttp
import anyio
import requests

from web_upload_fixture import Fixture, Json, server_for, wire

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('source_web_upload', ROOT / 'openpilot/selfdrive/carrot/web_upload.py')
assert SPEC is not None and SPEC.loader is not None
SOURCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOURCE)


def original(command: dict[str, Json]) -> dict[str, Json]:
  progress: list[Json] = []
  checks = 0
  sent = 0

  def cancel() -> bool:
    nonlocal checks
    checks += 1
    return (command.get('cancel_after') is not None and checks >= command['cancel_after']) or (
      command.get('cancel_sent') is not None and sent >= command['cancel_sent']
    )

  def callback(filename: str, count: int, size: int, chunk: int) -> None:
    nonlocal sent
    sent = count
    progress.append([filename, count, size, chunk])
    if command.get('progress_failure') == len(progress):
      raise RuntimeError('progress callback failed')

  try:
    match command['op']:
      case 'normalize':
        result = SOURCE.normalize_base_url(command['value'], command['default'])
      case 'helpers':
        env = command['env']
        names = {'upload_url': 'CARROT_WEB_UPLOAD_URL', 'upload_token': 'CARROT_WEB_UPLOAD_TOKEN', 'tmux_url': 'CARROT_TMUX_WEB_UPLOAD_URL'}
        saved = {name: os.environ.get(name) for name in names.values()}
        try:
          for key, name in names.items():
            os.environ[name] = env.get(key, '')
          target = SOURCE.tmux_web_target(command['settings'], command['session_token'])
          logs = SOURCE.carrot_logs_web_target()
          result = {
            'settings': list(SOURCE.web_upload_settings(command['settings'])),
            'api_url': SOURCE.api_url(command['base'], *command['parts']),
            'tmux': {'url': target[0], 'headers': target[1]},
            'carrot_logs': {'url': logs[0], 'headers': logs[1]},
            'device_id': SOURCE.upload_device_id(command['metadata']),
            'session_payload': SOURCE._session_payload(command['metadata'], command['purpose']),
          }
        finally:
          for name, value in saved.items():
            if value is None:
              os.environ.pop(name, None)
            else:
              os.environ[name] = value
      case 'session':
        if command['sync']:
          result = SOURCE.create_web_upload_session_sync(command['base'], command['metadata'], requests.post, command['purpose'])
        else:
          result = anyio.run(SOURCE.create_web_upload_session, command['base'], command['metadata'], command['purpose'])
      case 'health':
        result = anyio.run(SOURCE.check_web_upload_health, command['base'], command['token'])
      case 'complete':
        result = anyio.run(SOURCE.send_web_upload_complete, command['base'], command['token'], command['payload'])
      case 'folder':
        result = anyio.run(
          partial(
            SOURCE.upload_folder_to_web,
            command['folder'],
            command['directory'],
            command['remote_path'],
            command['base'],
            command['token'],
            should_cancel=cancel,
            filenames=command.get('filenames'),
            on_progress=callback,
          )
        )
      case 'tmux':
        response = SOURCE.post_tmux_web(
          command['url'], command['headers'], command['payload'], command['tmux_path'], command.get('settings_path'), requests.post
        )
        result = {'status': response.status_code, 'body': response.text}
      case _:
        raise AssertionError(command['op'])
    output = {'result': result}
  except (RuntimeError, ValueError, AttributeError, OSError, TypeError, requests.RequestException, aiohttp.ClientError) as error:
    output = {'error': str(error), 'error_type': type(error).__name__}
  output.update(progress=progress, cancel_checks=checks)
  return output


def compare(source: dict[str, Json], native: dict[str, Json], loose: bool) -> None:
  for output in (source, native):
    result = output.get('result')
    if isinstance(result, dict):
      result.pop('elapsed_ms', None)
  if loose:
    assert bool(source.get('error')) == bool(native.get('error')), (source, native)
    assert source['progress'] == native['progress'], (source, native)
    return
  assert source == native, (source, native)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  cases: list[dict[str, Json]] = []
  base = 'http://127.0.0.1:1'
  process = subprocess.Popen([str(args.binary.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  assert process.stdin is not None and process.stdout is not None

  def run(name: str, command: dict[str, Json], plans: list[dict[str, Json]] | None = None, loose: bool = False, compare_wire: bool = True) -> None:
    outputs = []
    captures = []
    for language in ['python', 'rust']:
      fixture = Fixture(args.output, plans=plans or [], prefix=f'{name}-{language}')
      server = server_for(fixture)
      thread = threading.Thread(target=partial(server.serve_forever, poll_interval=0.01), daemon=True)
      thread.start()
      actual = command | {
        key: value.replace(base, f'http://127.0.0.1:{server.server_port}')
        for key, value in command.items()
        if key in {'base', 'url'} and isinstance(value, str)
      }
      try:
        if language == 'python':
          output = original(actual)
        else:
          process.stdin.write(json.dumps(actual) + '\n')
          process.stdin.flush()
          line = process.stdout.readline()
          if not line:
            raise AssertionError(process.stderr.read())
          output = json.loads(line)
      finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
      if command['op'] == 'helpers':
        output = json.loads(json.dumps(output).replace(f'http://127.0.0.1:{server.server_port}', base))
      outputs.append(output)
      captures.append(list(fixture.captures))
    source, native = outputs
    source_wire, native_wire = captures
    record = {'name': name, 'command': command, 'python': source, 'rust': native, 'python_requests': source_wire, 'rust_requests': native_wire}
    (args.output / f'{name}.json').write_text(json.dumps(record, indent=2))
    compare(source, native, loose)
    if command['op'] == 'folder':
      assert all(item['method'] in {'PUT', 'GET'} and '/complete' not in str(item['path']) for item in source_wire + native_wire)
    if compare_wire:
      assert wire(source_wire) == wire(native_wire), (name, wire(source_wire), wire(native_wire))
    cases.append({'name': name, 'artifact': str(args.output / f'{name}.json'), 'passed': True})

  try:
    with tempfile.TemporaryDirectory(prefix='web-upload-') as temporary:
      root = Path(temporary)
      (root / 'rlog.zst').write_bytes(bytes(range(256)) * 9000 + b'end')
      (root / 'qcamera.ts').write_bytes(b'camera\x00\xff')
      (root / 'zero').touch()
      (root / 'link').symlink_to(root / 'rlog.zst')
      (root / 'subdir').mkdir()
      folder = {
        'op': 'folder',
        'folder': str(root),
        'base': base,
        'token': 'synthetic-token',
        'directory': 'car name/id',
        'remote_path': 'route|0',
        'filenames': ['rlog.zst'],
      }
      for index, (value, default) in enumerate(
        [(' https://test/// ', ''), ('', 'http://fallback/'), ('HTTPS://test', ''), ('file:///tmp/x', ''), ('', ''), ('http://', '')]
      ):
        run(f'normalize-{index}', {'op': 'normalize', 'value': value, 'default': default})
      run(
        'helpers',
        {
          'op': 'helpers',
          'settings': {'toss_upload_url': 'https://legacy.test/'},
          'env': {'upload_token': ' token '},
          'metadata': {
            'dongleId': 'UNKNOWN',
            'serial': ' sample ',
            'false': False,
            'count': 17,
            'items': ['a', True, '\u00a0', '\u200b', '\x00'],
            'wide': '한😀' * 100,
          },
          'base': base,
          'parts': ['upload', '한글 /', '%?#|'],
          'session_token': ' ',
          'purpose': 'dashcam',
        },
      )
      run('folder-stream', folder)
      run('folder-auto', folder | {'filenames': None})
      run('folder-order-dedup', folder | {'filenames': ['qcamera.ts', 'rlog.zst', 'qcamera.ts']})
      run('folder-explicit-symlink', folder | {'filenames': ['link']})
      run('folder-zero', folder | {'filenames': ['zero']})
      run('folder-empty', folder | {'filenames': []})
      run('folder-token', folder | {'token': ''})
      for index, names in enumerate([[''], ['..'], ['../secret'], ['x\\y'], ['missing'], ['subdir']]):
        run(f'folder-invalid-{index}', folder | {'token': '', 'filenames': names})
      for count in [1, 2]:
        run(f'folder-progress-failure-{count}', folder | {'progress_failure': count}, compare_wire=False)
      run('folder-disconnect', folder, [{'disconnect': True}], loose=True)
      run('folder-invalid-json', folder, [{'text': 'not json'}])
      run('folder-size-retry', folder, [{'size_delta': 1}, {}])
      run('folder-size-fail', folder, [{'size_delta': -1}])
      for status in [200, 201, 202, 204, 299, 400, 401, 403, 412, 500]:
        run(f'folder-status-{status}', folder, [{'status': status}])
      for index, body in enumerate(
        [
          [],
          ['truthy'],
          'text',
          None,
          {'ok': False},
          {'ok': True},
          {'ok': True, 'size': 'bad'},
          {'ok': True, 'size': 2304003.9},
          {'ok': True, 'size': '2_304_003'},
          {'ok': True, 'size': []},
          {'ok': True, 'size': '_2304003'},
          {'ok': True, 'size': '٢٣٠٤٠٠٣'},
          {'ok': True, 'size': '2__304_003'},
        ]
      ):
        run(f'folder-body-{index}', folder, [{'body': body}])
      run('folder-partial', folder | {'filenames': ['qcamera.ts', 'rlog.zst', 'zero']}, [{}, {'status': 500}, {'status': 500}])
      for count in [1, 2, 3, 4, 5, 6, 7, 8]:
        run(f'folder-cancel-{count}', folder | {'cancel_after': count}, compare_wire=count == 8)
      run('folder-cancel-progress', folder | {'cancel_sent': 1048576}, compare_wire=False)
      for op in ['session', 'health', 'complete']:
        command = {
          'op': op,
          'base': base,
          'token': 'synthetic-token',
          'metadata': {'dongle_id': 'synthetic', 'nested': {'x': 1}},
          'purpose': 'dashcam',
          'sync': False,
          'payload': {'result': 'synthetic', 'unicode': '한글'},
        }
        for status in [200, 201, 204, 400, 401, 500]:
          run(f'{op}-{status}', command, [{'status': status}])
        if op == 'session':
          for index, body in enumerate(
            [{'ok': True, 'token': ' '}, {'ok': True, 'token': 123}, ['bad'], [], None, {'ok': True}, {'ok': False, 'error': '한' * 400}]
          ):
            run(f'session-body-{index}', command, [{'body': body}])
          run('session-sync', command | {'sync': True, 'purpose': 'tmux'})
      run('complete-no-token', {'op': 'complete', 'base': 'bad', 'token': '', 'payload': {}})
      tmux = {
        'op': 'tmux',
        'url': base + '/tmux',
        'headers': {'Authorization': 'Bearer synthetic'},
        'payload': {'reason': 'test', 'values': ['a', 'b'], 'flag': True, 'absent': None, 'q"\n': '한'},
        'tmux_path': str(root / 'qcamera.ts'),
        'settings_path': str(root / 'zero'),
      }
      run('tmux-multipart', tmux)
      run('tmux-without-settings', tmux | {'settings_path': str(root / 'missing')})
      run('tmux-error-status', tmux, [{'status': 403}])
      for status in [301, 302, 303, 307, 308]:
        run(f'tmux-redirect-{status}', tmux, [{'status': status, 'location': '/tmux-final'}, {}])
      for status in [301, 302, 303, 307, 308]:
        run(f'folder-redirect-{status}', folder | {'filenames': ['qcamera.ts']}, [{'status': status, 'location': '/redirected'}, {}])
      (args.output / 'report.json').write_text(json.dumps({'passed': len(cases), 'cases': cases}, indent=2))
      print(json.dumps({'passed': len(cases), 'report': str(args.output / 'report.json')}))
  finally:
    process.stdin.close()
    process.wait(timeout=5)


if __name__ == '__main__':
  main()

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
from urllib.parse import parse_qs, urlparse

import zstandard

from openpilot.cereal import log
from check_uploader import endpoint, signing_key


def wait_for(predicate, processes, deadline=10):
  end = time.monotonic() + deadline
  while not predicate():
    assert all(process.poll() is None for process in processes), 'native service exited before readiness'
    assert time.monotonic() < end, 'native service readiness deadline'
    time.sleep(0.02)


def run(binaries: Path, output: Path) -> None:
  output.mkdir(parents=True)
  prefix = f'-runtime-upload-{os.getpid()}-{time.time_ns()}'
  namespace = Path('/dev/shm') / f'msgq_{prefix}'
  namespace.mkdir()
  log_socket = Path('/tmp') / f'logmessage{prefix}'
  children = []
  try:
    with tempfile.TemporaryDirectory(prefix='runtime-upload-') as temporary:
      root = Path(temporary)
      params = root / 'params' / prefix
      params.mkdir(parents=True)
      (params / 'DongleId').write_text('0000000000000000')
      (params / 'IsOffroad').write_text('1')
      (params / 'GitCommit').write_text('owned-runtime-upload')
      (root / 'build.json').write_text(json.dumps({'openpilot': {'git_origin': 'fixture/owned'}}))
      routes = output / 'routes'
      routes.mkdir()
      environment = {
        **os.environ,
        'HOME': str(root),
        'OPENPILOT_PREFIX': prefix,
        'PARAMS_ROOT': str(params.parent),
        'LOG_ROOT': str(routes),
        'OPENPILOT_BASEDIR': str(root),
        'OPENPILOT_ROOT': str(root),
        'RUST_BACKTRACE': '1',
      }
      for name in ('ZMQ', 'CEREAL_FAKE', 'MANAGER_DAEMON', 'OPENPILOT_LOG_CONTEXT', 'FAKEUPLOAD'):
        environment.pop(name, None)
      for binary, arguments in [('openpilot-logmessaged', ['--log-root', str(output / 'swaglog')]), ('openpilot-loggerd', [])]:
        with (output / f'{binary}.log').open('wb') as stream:
          process = subprocess.Popen([str(binaries / binary), *arguments], env=environment, stdout=stream, stderr=stream)
        children.append(process)
        if binary == 'openpilot-logmessaged':
          wait_for(lambda: log_socket.exists() and (namespace / 'errorLogMessage').exists(), children)
      wait_for(lambda: (params / 'CurrentRoute').exists(), children)
      managed_environment = {
        **environment,
        'MANAGER_DAEMON': 'uploader',
        'OPENPILOT_LOG_CONTEXT': json.dumps({'dongle_id': 'owned-device', 'version': 'owned-version'}),
      }
      failure = subprocess.run(
        [str(binaries / 'openpilot-uploader'), '--invalid-managed-fixture'], env=managed_environment, capture_output=True, timeout=10, check=False
      )
      (output / 'managed-failure.log').write_bytes(failure.stdout + failure.stderr)
      assert failure.returncode == 1 and b'crash\n' in failure.stderr
      time.sleep(0.3)
      children[1].send_signal(signal.SIGTERM)
      assert children[1].wait(timeout=5) == 0
      children[0].send_signal(signal.SIGTERM)
      assert children[0].wait(timeout=5) == 0
      qlogs = list(routes.glob('*/qlog.zst'))
      assert len(qlogs) == 1, qlogs
      qlog = qlogs[0]
      assert not list(routes.rglob('*.lock'))
      captures = []
      signing_key(root / f'.comma{prefix}' / 'persist', 'id_ecdsa')
      with endpoint({}, captures) as host:
        uploaded = subprocess.run(
          [str(binaries / 'openpilot-uploader'), '--cycles', '1'],
          env={**environment, 'API_HOST': host, 'FORCEWIFI': '1', 'UPLOADER_SLEEP': '0'},
          capture_output=True,
          timeout=15,
          check=False,
        )
      (output / 'uploader.log').write_bytes(uploaded.stdout + uploaded.stderr)
      assert uploaded.returncode == 0, uploaded.stderr
      puts = [row for row in captures if row[0] == 'PUT']
      gets = [row for row in captures if row[0] == 'GET']
      assert len(puts) == len(gets) == 1, [(row[0], row[1]) for row in captures]
      assert parse_qs(urlparse(gets[0][1]).query)['path'] == [str(qlog.relative_to(routes))]
      assert puts[0][3] == qlog.read_bytes()
      assert os.getxattr(qlog, 'user.upload') == b'1'
      content = zstandard.ZstdDecompressor().stream_reader(io.BytesIO(puts[0][3])).read()
      messages = list(log.Event.read_multiple_bytes(content))
      errors = [json.loads(str(event.errorLogMessage)) for event in messages if event.which() == 'errorLogMessage']
      crashes = [row for row in errors if row.get('msg') == 'crash']
      assert len(crashes) == 1, errors
      crash = crashes[0]
      assert crash['ctx']['runtime_language'] == 'rust'
      assert crash['ctx']['runtime_implementation'] == 'openpilot-uploader'
      assert crash['ctx']['daemon'] == 'uploader' and crash['ctx']['dongle_id'] == 'owned-device'
      commit = crash['ctx']['source_commit']
      assert len(commit) == 40 and all(char in '0123456789abcdef' for char in commit)
      assert crash['ctx']['source_tree'] in ('clean', 'dirty')
      assert 'openpilot_uploader::' in crash['exc_info']
      receipt = {
        'result': 'PASS',
        'upload_bytes': len(puts[0][3]),
        'uploaded_sha256': hashlib.sha256(puts[0][3]).hexdigest(),
        'route_messages': len(messages),
        'typed_crash_records': len(crashes),
        'upload_marked': True,
        'source_commit': commit,
        'source_tree': crash['ctx']['source_tree'],
        'binaries': {
          name: hashlib.sha256((binaries / name).read_bytes()).hexdigest() for name in ('openpilot-logmessaged', 'openpilot-loggerd', 'openpilot-uploader')
        },
        'scope': 'real native child, collector, route logger and uploader with owned storage and HTTP; no complete manager or device claim',
      }
      (output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
      print(json.dumps(receipt))
  finally:
    for process in reversed(children):
      if process.poll() is None:
        process.kill()
        process.wait()
    log_socket.unlink(missing_ok=True)
    shutil.rmtree(namespace)


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  options = parser.parse_args()
  run(options.binaries.resolve(), options.output.resolve())

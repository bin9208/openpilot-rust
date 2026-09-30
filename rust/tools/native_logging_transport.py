# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq==27.1.0"]
# ///
# Run via check_native_logging.py: original C++ and native transport scenarios.
import json
import os
import signal
import subprocess
import time
import uuid
from pathlib import Path

import zmq

from check_native_logging import Peer, receiver


def check(source: Path, native: Path, output: Path) -> dict:
  env = {k: v for k, v in os.environ.items() if k not in ['DONGLE_ID', 'GIT_ORIGIN', 'GIT_BRANCH', 'GIT_COMMIT', 'MANAGER_DAEMON', 'CLEAN', 'LOGPRINT']}
  report = {}
  for original, binary in [(True, source), (False, native)]:
    name = 'source' if original else 'rust'
    start = time.monotonic()
    peer = Peer(binary, output / name / 'backpressure', f'ipc:///tmp/absent-{uuid.uuid4().hex}', env, original=original)
    response = peer.command({'op': 'flood', 'count': 5000})
    assert response['sent'] > 0 and response['dropped'] > 0 and response['sent'] + response['dropped'] == 5000
    peer.finish()
    duration = time.monotonic() - start
    assert duration < 2, duration
    report[name] = {'backpressure': response, 'send_and_shutdown_seconds': duration}
    with receiver() as (endpoint, pull):
      peer = Peer(binary, output / name / 'threads', endpoint, env, original=original)
      response = peer.command({'op': 'threads', 'count': 64})
      assert response['sent'] == 64 and response['dropped'] == 0
      messages = [json.loads(peer.receive(pull)[1:])['msg'] for _ in range(64)]
      assert set(messages) == {f'thread-{i}' for i in range(64)} and not pull.poll(50)
      if not original:
        assert peer.command({'op': 'close'}) == {'sent': 0, 'dropped': 0, 'filtered': 0}
      peer.finish()
    with receiver() as (endpoint, pull):
      peer = Peer(binary, output / name / 'console-failure', endpoint, env, original=original, full=True)
      response = peer.command({'op': 'emit', 'level': 40, 'text': 'console failure remains nonfatal'})
      assert response['sent'] == 1
      assert json.loads(peer.receive(pull)[1:])['msg'] == 'console failure remains nonfatal'
      peer.finish()
    command = {'op': 'emit', 'level': 40, 'text': ''}
    completed = subprocess.run([str(binary)] + ([] if original else ['invalid://endpoint']), input=json.dumps(command) + '\n',
                               env={**env, 'NATIVE_LOG_ENDPOINT': 'invalid://endpoint'}, capture_output=True, text=True, timeout=5)
    assert completed.returncode == 0 and json.loads(completed.stderr)['sent'] == 0
    (output / name / 'empty-invalid-endpoint.json').write_text(json.dumps({'returncode': completed.returncode, 'stderr': completed.stderr}) + '\n')
    command['text'] = 'invalid endpoint'
    completed = subprocess.run([str(binary)] + ([] if original else ['invalid://endpoint']), input=json.dumps(command) + '\n',
                               env={**env, 'NATIVE_LOG_ENDPOINT': 'invalid://endpoint'}, capture_output=True, text=True, timeout=5)
    if original:
      assert completed.returncode == 0 and json.loads(completed.stderr)['dropped'] == 1
    else:
      assert completed.returncode == 1 and 'Transport(' in completed.stderr
    (output / name / 'nonempty-invalid-endpoint.json').write_text(json.dumps({'returncode': completed.returncode, 'stderr': completed.stderr}) + '\n')
    for sig in [signal.SIGINT, signal.SIGTERM]:
      directory = output / name / sig.name
      peer = Peer(binary, directory, f'ipc:///tmp/absent-{uuid.uuid4().hex}', env, original=original)
      peer.process.stdin.write(json.dumps({'op': 'flood', 'count': 10000000}) + '\n')
      peer.process.stdin.flush()
      time.sleep(0.05)
      start = time.monotonic()
      peer.process.send_signal(sig)
      code = peer.process.wait(timeout=2)
      assert code == -sig
      peer.stdout.close()
      (directory / 'signal.json').write_text(json.dumps({'signal': sig.name, 'returncode': code, 'seconds': time.monotonic() - start}) + '\n')
  with zmq.Context() as context, context.socket(zmq.PULL) as pull:
    pull.linger = 0
    prefix = '-' + uuid.uuid4().hex
    pull.bind(f'ipc:///tmp/logmessage{prefix}')
    peer = Peer(native, output / 'runtime-prefix', 'runtime', {**env, 'OPENPILOT_PREFIX': prefix}, original=False)
    assert peer.command({'op': 'emit', 'level': 20, 'text': 'runtime endpoint'})['sent'] == 1
    assert json.loads(peer.receive(pull)[1:])['msg'] == 'runtime endpoint'
    assert peer.command({'op': 'close'})['sent'] == 0
    peer.finish()
  report.update(thread_messages_each=64, console_failure='IPC delivered and both producers returned successfully',
                empty='invalid endpoint remains uninitialized', runtime_prefix='received at normal prefixed runtime IPC',
                transport_error='source ignores invalid connect then drops; Rust returns a typed transport error (caller must handle best effort)',
                signals='source/native default SIGINT and SIGTERM terminate during backpressure')
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report

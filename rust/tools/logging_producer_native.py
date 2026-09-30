"""Native protocol/context/collector surfaces for the Rust structured producer."""

from __future__ import annotations
from contextlib import contextmanager
import json
from pathlib import Path
import signal
import socket
import tempfile
import time
import zmq
from logging_producer_reference import ROOT, console_handler, probe, source


@contextmanager
def receiver():
  context = zmq.Context()
  with tempfile.TemporaryDirectory(prefix='logging-producer-') as directory:
    endpoint = 'ipc://' + directory + '/socket'
    pull = context.socket(zmq.PULL)
    pull.setsockopt(zmq.RCVTIMEO, 5000)
    pull.setsockopt(zmq.LINGER, 0)
    pull.bind(endpoint)
    try:
      yield endpoint, pull
    finally:
      pull.close()
      context.term()


def receive(pull, client, label):
  raw = pull.recv()
  (client.output / f'{label}.packet').write_bytes(raw)
  value = json.loads(raw[1:])
  assert value['levelnum'] == raw[0]
  assert value['name'] == 'swaglog' and value['filename'] == 'logging_probe.rs'
  assert value['process'] == client.process.pid and value['host'] == socket.gethostname()
  assert abs(value['created'] - time.clock_gettime(time.CLOCK_REALTIME)) < 5
  assert value['module'] == 'logging_probe' and value['funcName'].startswith('logging_probe::')
  path = Path(value['pathname'])
  candidates = [ROOT / path, ROOT / 'rust' / path]
  source_path = next(p for p in candidates if p.is_file())
  assert 'log_site!' in source_path.read_text().splitlines()[value['lineno'] - 1]
  return value


def native(binary: Path, output: Path) -> dict:
  output.mkdir(parents=True)
  logger, _ = source()
  scopes = []
  counts = {'packets': 0, 'contexts': 0}
  with receiver() as (endpoint, pull), probe(binary, output / 'probe', endpoint) as client:
    identity = json.loads(client.command({'action': 'snapshot'})['context'])
    assert identity['runtime_language'] == 'rust' and len(identity['source_commit']) == 40
    assert identity['source_tree'] in ('clean', 'dirty', 'unknown')
    logger.bind_global(**identity)
    for action, fields in [
      ('bind', [['key', 'local'], ['ordered', 1]]),
      ('global', [['key', 'global'], ['shared', True]]),
      ('push', [['ordered', 2], ['nested', 1]]),
      ('bind', [['transient', 3]]),
      ('push', [['nested', 2]]),
      ('pop', []),
      ('pop', []),
    ]:
      command = {'action': action}
      if action != 'pop':
        command['fields'] = fields
      assert client.command(command) == {'ok': True}
      match action:
        case 'bind':
          logger.bind(**dict(fields))
        case 'global':
          logger.bind_global(**dict(fields))
        case 'push':
          scope = logger.ctx(**dict(fields))
          scope.__enter__()
          scopes.append(scope)
        case 'pop':
          scopes.pop().__exit__(None, None, None)
      assert client.command({'action': 'snapshot'})['context'] == json.dumps(logger.get_ctx())
      counts['contexts'] += 1
    before = client.command({'action': 'snapshot'})['context']
    result = client.command({'action': 'panic_scope'})
    assert result['panicked'] and result['context'] == before
    for level in (0, 10, 20, 30, 40, 50):
      result = client.command({'action': 'emit', 'level': level, 'text': f'level-{level}', 'exception': None})
      if level == 0:
        assert result['delivery'] == 'filtered'
        continue
      assert result['delivery'] == 'sent'
      value = receive(pull, client, f'level-{level}')
      assert value['msg'] == f'level-{level}' and value['ctx'] == logger.get_ctx()
      assert value['thread'] == client.process.pid
      assert value['threadName'] == Path(f'/proc/{client.process.pid}/comm').read_text().strip()
      counts['packets'] += 1
    for fields in ([['debug', False]], [['error', False]], [['debug', True], ['error', None]]):
      assert client.command({'action': 'emit_event', 'name': 'event', 'arguments': [1, 'x'], 'fields': fields, 'special': False})['delivery'] == 'sent'
      value = receive(pull, client, f'event-{counts["packets"]}')
      assert value['levelnum'] == (40 if 'error' in dict(fields) else 10)
      assert value['msg'] == dict(event='event', args=[1, 'x'], **dict(fields))
      counts['packets'] += 1
    client.write({'action': 'thread', 'name': 'producer-worker', 'fields': [['thread_local', 'worker']]})
    value = receive(pull, client, 'worker')
    assert value['thread'] != client.process.pid
    assert value['threadName'] == Path(f'/proc/{client.process.pid}/task/{value["thread"]}/comm').read_text().strip() == 'producer-worker'
    assert value['ctx'] == dict(thread_local='worker', **logger.global_ctx)
    client.process.stdin.write('\n')
    client.process.stdin.flush()
    assert client.receive()['delivery'] == 'sent'
    assert client.command({'action': 'snapshot'})['context'] == before
    counts['packets'] += 1
    assert client.command({'action': 'close'}) == {'ok': True}
    assert 'error' in client.command({'action': 'emit', 'level': 20, 'text': 'after-close', 'exception': None})
    client.finish()
  report = {'result': 'pass', **counts, 'identity': identity, 'native_pid_tid_threadname_and_rust_callsite': True}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def backpressure(binary: Path, output: Path) -> dict:
  output.mkdir(parents=True)
  with tempfile.TemporaryDirectory(prefix='logging-no-receiver-') as directory, probe(binary, output / 'probe', 'ipc://' + directory + '/absent') as client:
    result = client.command({'action': 'flood', 'count': 5000})
    assert result['sent'] > 0 and result['dropped'] > 0 and result['sent'] + result['dropped'] == 5000
    assert result['seconds'] < 2
    started = time.monotonic()
    client.command({'action': 'close'})
    elapsed = time.monotonic() - started
    assert elapsed < 2
    client.finish()
  with probe(binary, output / 'invalid-endpoint', 'bad-protocol://invalid') as client:
    error = client.command({'action': 'emit', 'level': 20, 'text': 'typed error', 'exception': None})
    assert 'error' in error
    client.finish()
  signals = []
  for signum in (signal.SIGINT, signal.SIGTERM):
    with tempfile.TemporaryDirectory(prefix='logging-signal-') as directory, probe(binary, output / signum.name, 'ipc://' + directory + '/absent') as client:
      client.write({'action': 'flood', 'count': 10000000})
      time.sleep(0.05)
      start = time.monotonic()
      client.process.send_signal(signum)
      assert client.process.wait(timeout=2) == -signum
      signals.append({'signal': signum.name, 'seconds': time.monotonic() - start, 'exit': -signum})
  from logging_producer_transport import original_backpressure

  original = original_backpressure(output / 'original')
  assert (result['sent'], result['dropped']) == (original['sent'], original['dropped'])
  report = {
    'result': 'pass',
    **result,
    'source': original,
    'close_seconds': elapsed,
    'signals': signals,
    'signal_scope': 'producer installs no global handler; default process signals remain interruptible',
  }
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def console(binary: Path, output: Path) -> list[dict]:
  results = []
  for index, setting in enumerate((None, 'debug', 'info', 'warning', 'error', '', 'WARNING')):
    logger, _ = source()
    handler, text = console_handler(setting)
    logger.addHandler(handler)
    with (
      tempfile.TemporaryDirectory(prefix='logging-console-') as directory,
      probe(binary, output / str(index), 'ipc://' + directory + '/absent', setting) as client,
    ):
      for level in (10, 20, 30, 40, 50):
        client.command({'action': 'emit', 'level': level, 'text': f'console-{level}', 'exception': None})
        logger.log(level, f'console-{level}')
      client.command({'action': 'emit_event', 'name': 'structured', 'arguments': [], 'fields': [['error', False], ['text', '한글']], 'special': False})
      logger.event('structured', error=False, text='한글')
      client.command({'action': 'emit', 'level': 40, 'text': 'exception', 'exception': 'ValueError: fixture'})
      logger.error('exception', exc_info=(ValueError, ValueError('fixture'), None))
      client.finish()
    assert (client.output / 'stderr.log').read_text() == text.getvalue(), setting
    results.append({'LOGPRINT': setting, 'console_bytes': len(text.getvalue().encode()), 'result': 'pass'})
  return results

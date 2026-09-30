"""Fork lifecycle and both original/Rust collector compatibility using real transports."""

from __future__ import annotations
import json
import os
from pathlib import Path
import signal
import subprocess
import time
from logging_producer_native import receiver
from logging_producer_reference import probe
from logmessaged_native import Peer


def forks(binary: Path, output: Path) -> list[dict]:
  results = []
  for reconnect in (True, False):
    destination = output / ('reconnect' if reconnect else 'drop')
    destination.mkdir(parents=True)
    with receiver() as (endpoint, pull), (destination / 'stderr.log').open('w') as stderr:
      process = subprocess.Popen([binary, endpoint, *([] if reconnect else ['drop'])], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
      assert process.stdin is not None
      records = []
      try:
        for expected in ['parent-before', 'child-record', 'parent-after'] if reconnect else ['parent-before', 'parent-after']:
          raw = pull.recv()
          (destination / f'{expected}.packet').write_bytes(raw)
          value = json.loads(raw[1:])
          assert raw[0] == 20 and value['msg'] == expected
          assert value['filename'] == 'logging_fork.rs' and value['ctx']['runtime_language'] == 'rust'
          if expected == 'child-record':
            assert value['process'] != process.pid and value['thread'] == value['process']
            status = Path(f'/proc/{value["process"]}/status').read_text()
            assert f'PPid:\t{process.pid}\n' in status
          else:
            assert value['process'] == process.pid and value['thread'] == process.pid
          records.append({'label': expected, 'pid': value['process'], 'tid': value['thread']})
          process.stdin.write('received\n')
          process.stdin.flush()
        start = time.monotonic()
        assert process.wait(timeout=2) == 0
        results.append(
          {'mode': 'reconnect' if reconnect else 'drop-inherited', 'result': 'pass', 'close_seconds': time.monotonic() - start, 'records': records}
        )
      finally:
        if process.poll() is None:
          process.kill()
          process.wait(timeout=5)
  (output / 'report.json').write_text(json.dumps(results, indent=2) + '\n')
  return results


def collectors(binary: Path, collector: Path, output: Path) -> list[dict]:
  results = []
  for original in (True, False):
    destination = output / ('original' if original else 'rust')
    peer = Peer(collector, destination, original)
    try:
      peer.start()
      with receiver() as (endpoint, pull), probe(binary, destination / 'producer', endpoint) as client:
        for index, fields in enumerate(([], [['debug', False]], [['error', False]], [['debug', False], ['error', None]])):
          result = client.command(
            {'action': 'emit_event', 'name': 'runtime_probe', 'arguments': [], 'fields': fields + [['value', 1.25], ['text', '한글😀']], 'special': False}
          )
          assert result['delivery'] == 'sent'
          raw = pull.recv()
          peer.send([raw], f'record-{index}')
        client.finish()
      code, seconds = peer.stop()
      assert code in (0, -signal.SIGINT) if original else code == 0
      disk = []
      for path in sorted(peer.root.glob('swaglog.*')):
        disk += [json.loads(line) for line in path.read_text().splitlines()]
      assert len(disk) == 3
      assert all(value['ctx']['runtime_language'] == 'rust' and value['msg']['event$s'] == 'runtime_probe' for value in disk)
      assert len(peer.records['logMessage']) == 4 and len(peer.records['errorLogMessage']) == 2
      result = {
        'collector': 'original' if original else 'rust',
        'result': 'pass',
        'log_messages': 4,
        'error_messages': 2,
        'disk_records': 3,
        'exit': code,
        'shutdown_seconds': seconds,
      }
      (destination / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
      results.append(result)
    finally:
      peer.close()
  return results


def original_backpressure(output: Path) -> dict:
  import logging
  import tempfile
  import zmq
  from logging_producer_reference import original_socket_handler, source

  output.mkdir(parents=True)
  logger, _ = source()
  with tempfile.TemporaryDirectory(prefix='original-log-pressure-') as directory:
    handler = original_socket_handler('ipc://' + directory + '/absent', logger)
    handler.connect()
    assert handler.sock.getsockopt(zmq.LINGER) == 10

    class ObservedSocket:
      def __init__(self, socket):
        self.socket = socket
        self.sent = self.dropped = 0

      def send(self, data, flags):
        assert flags == zmq.NOBLOCK and data[0] == logging.DEBUG
        try:
          self.socket.send(data, flags)
          self.sent += 1
        except zmq.Again:
          self.dropped += 1
          raise

      def close(self):
        self.socket.close()

    observed = ObservedSocket(handler.sock)
    handler.sock = observed
    record = logging.LogRecord('swaglog', logging.DEBUG, __file__, 1, 'backpressure', (), None)
    for _ in range(5000):
      handler.emit(record)
    handler.close()
  with receiver() as (endpoint, pull):
    handler = original_socket_handler(endpoint, logger)
    handler.emit(record)
    raw = pull.recv()
    assert raw[0] == 10 and json.loads(raw[1:])['msg'] == 'backpressure'
    (output / 'source.packet').write_bytes(raw)
    handler.close()
  report = {'result': 'pass', 'sent': observed.sent, 'dropped': observed.dropped, 'linger_ms': 10, 'noblock': True}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def runtime_endpoint(binary: Path, output: Path) -> dict:
  import tempfile
  import zmq
  from logging_producer_native import receive

  with tempfile.TemporaryDirectory(prefix='logging-runtime-') as directory:
    prefix = Path(directory).name
    endpoint = 'ipc:///tmp/logmessage' + prefix
    previous = os.environ.get('OPENPILOT_PREFIX')
    context = zmq.Context()
    pull = context.socket(zmq.PULL)
    pull.setsockopt(zmq.RCVTIMEO, 5000)
    pull.setsockopt(zmq.LINGER, 0)
    pull.bind(endpoint)
    try:
      os.environ['OPENPILOT_PREFIX'] = prefix
      with probe(binary, output) as client:
        assert client.command({'action': 'emit', 'level': 20, 'text': 'runtime-endpoint', 'exception': None})['delivery'] == 'sent'
        value = receive(pull, client, 'runtime-endpoint')
        assert value['msg'] == 'runtime-endpoint'
        client.finish()
    finally:
      if previous is None:
        os.environ.pop('OPENPILOT_PREFIX', None)
      else:
        os.environ['OPENPILOT_PREFIX'] = previous
      pull.close()
      context.term()
      Path(endpoint.removeprefix('ipc://')).unlink(missing_ok=True)
  report = {'result': 'pass', 'endpoint': endpoint, 'uses_runtime_prefix': True}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report

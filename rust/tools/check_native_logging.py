#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq==27.1.0"]
# ///
# Run: uv run rust/tools/check_native_logging.py --binary rust/target/debug/examples/native_logging_probe --output /tmp/native-logging
import argparse
import base64
import contextlib
import hashlib
import json
import os
import re
import selectors
import subprocess
import time
import uuid
from pathlib import Path

import zmq

from native_logging_build import build


class Peer:
  def __init__(self, binary: Path, directory: Path, endpoint: str, env: dict[str, str], *, original: bool, full: bool = False):
    directory.mkdir(parents=True, exist_ok=True)
    self.directory, self.original = directory, original
    self.started = time.clock_gettime(time.CLOCK_REALTIME)
    self.commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    self.tree = 'dirty' if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal']) else 'clean'
    self.stdout = (Path('/dev/full') if full else directory / 'stdout.txt').open('wb')
    self.process = subprocess.Popen([str(binary)] + ([] if original else [endpoint]), stdin=subprocess.PIPE,
                                    stdout=self.stdout, stderr=subprocess.PIPE, text=True,
                                    env={**env, 'NATIVE_LOG_ENDPOINT': endpoint})
    self.responses = []
    self.packets = []

  def command(self, value: dict) -> dict:
    if self.original and value['op'] == 'rate':
      value = {**value, 'timestamp': str(value['timestamp'])}
    self.process.stdin.write(json.dumps(value) + '\n')
    self.process.stdin.flush()
    with selectors.DefaultSelector() as selector:
      selector.register(self.process.stderr, selectors.EVENT_READ)
      assert selector.select(5), 'producer command timed out'
    response = self.process.stderr.readline()
    assert response, self.process.poll()
    result = json.loads(response)
    self.responses.append({'command': value, 'response': result})
    return result

  def receive(self, pull: zmq.Socket) -> bytes:
    assert pull.poll(3000), 'no native packet'
    packet = pull.recv()
    record = json.loads(packet[1:])
    assert record['levelnum'] == packet[0]
    assert self.started <= record['created'] <= time.clock_gettime(time.CLOCK_REALTIME)
    assert set(record) == {'created', 'ctx', 'filename', 'funcname', 'levelnum', 'lineno', 'msg'}
    source = Path(record['filename'])
    if not source.is_absolute():
      source = Path('rust') / source
    line = source.read_text().splitlines()[record['lineno'] - 1]
    assert ('cloudlog' if self.original else 'logger.emit') in line, (record, line)
    assert record['funcname'] in (['emit', 'rate'] if self.original else ['native_logging_probe::emit'])
    if not self.original:
      ctx = record['ctx']
      assert ctx['runtime_language'] == 'rust' and ctx['source_commit'] == self.commit
      assert ctx['source_tree'] == self.tree
    self.packets.append({'raw_base64': base64.b64encode(packet).decode(), 'record': record})
    return packet

  def finish(self) -> None:
    self.process.stdin.close()
    assert self.process.wait(timeout=5) == 0
    assert self.process.stderr.read() == ''
    self.stdout.close()
    (self.directory / 'responses.json').write_text(json.dumps(self.responses, indent=2) + '\n')
    (self.directory / 'packets.json').write_text(json.dumps(self.packets, ensure_ascii=False, indent=2) + '\n')


@contextlib.contextmanager
def receiver():
  with zmq.Context() as context, context.socket(zmq.PULL) as pull:
    pull.linger = 0
    endpoint = f'ipc:///tmp/native-log-{uuid.uuid4().hex}'
    pull.bind(endpoint)
    yield endpoint, pull


def normalize(packet: bytes) -> bytes:
  text = packet[1:].decode()
  text = re.sub(r'"created": [^,}]+', '"created": 0', text)
  for key in ['filename', 'funcname']:
    text = re.sub('"' + key + r'": "(?:[^"\\]|\\.)*"', '"' + key + '": "metadata"', text)
  text = re.sub(r'"lineno": \d+', '"lineno": 0', text)
  for key in ['runtime_language', 'source_commit', 'source_tree']:
    text = re.sub('"' + key + r'": "(?:[^"\\]|\\.)*", ', '', text)
  return packet[:1] + text.encode()


def compare(source: Path, native: Path, output: Path) -> dict:
  env = {k: v for k, v in os.environ.items() if k not in ['DONGLE_ID', 'GIT_ORIGIN', 'GIT_BRANCH', 'GIT_COMMIT', 'MANAGER_DAEMON', 'CLEAN', 'LOGPRINT']}
  total = 0
  modes = [None, '', 'debug', 'info', 'warning', 'error', 'DEBUG']
  for i, mode in enumerate(modes):
    config = dict(env)
    if mode is not None:
      config['LOGPRINT'] = mode
    if i % 2:
      config.update(DONGLE_ID='', GIT_ORIGIN='fixture-origin', GIT_BRANCH='한글/branch', GIT_COMMIT='fixture-commit', MANAGER_DAEMON='fixture-daemon', CLEAN='')
    if i == 2:
      config['CLEAN'] = '0'
    records = []
    consoles = []
    for original, binary in [(True, source), (False, native)]:
      directory = output / f'console-{i}' / ('source' if original else 'rust')
      with receiver() as (endpoint, pull):
        peer = Peer(binary, directory, endpoint, config, original=original)
        packets = []
        for level in [0, 10, 20, 30, 40, 50]:
          for text in ['plain', '한글😀é\u2028\u2029\"\\\b\f\n\r\t\x01', '\0tail', 'prefix\0tail']:
            response = peer.command({'op': 'emit', 'level': level, 'text': text})
            assert response['sent'] == 1 and response['dropped'] == 0
            if original:
              assert response['linger'] == 100 and response['errors'] == 0
            packets.append(peer.receive(pull))
        response = peer.command({'op': 'emit', 'level': 40, 'text': ''})
        assert response['sent'] == 0 and not pull.poll(50)
        peer.finish()
        records.append([normalize(packet) for packet in packets])
        console = (directory / 'stdout.txt').read_bytes()
        filename = peer.packets[0]['record']['filename'].encode()
        consoles.append(console.replace(filename + b': ', b'FILE: '))
    assert records[0] == records[1], (i, [(a, b) for a, b in zip(*records, strict=True) if a != b][:1])
    assert consoles[0] == consoles[1], (i, consoles)
    total += len(records[0])
  rate_records = []
  timestamps = [0, 0, 1, 100_000_001, 100_000_002, 200_000_002, 300_000_002, 300_000_003]
  timestamps += [300_000_004 + i for i in range(500)] + [500_000_000, 500_000_001, 600_000_001, 600_000_002, 2**64 - 2, 2**64 - 1, 0, 1]
  for original, binary in [(True, source), (False, native)]:
    with receiver() as (endpoint, pull):
      peer = Peer(binary, output / 'rate' / ('source' if original else 'rust'), endpoint, env, original=original)
      packets = []
      for timestamp in timestamps:
        response = peer.command({'op': 'rate', 'timestamp': timestamp, 'text': f'rate-{timestamp}'})
        assert response['dropped'] == 0
        packets.extend(normalize(peer.receive(pull)) for _ in range(response['sent']))
      assert not pull.poll(50)
      peer.finish()
      rate_records.append(packets)
  assert rate_records[0] == rate_records[1]
  return {'wire_records': total, 'logprint_cases': len(modes), 'rate_inputs': len(timestamps), 'rate_records': len(rate_records[0])}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  native = args.binary.resolve()
  output = args.output.resolve()
  root = Path(__file__).resolve().parents[2]
  os.chdir(root)
  source = build(root, output / 'reference')
  report = compare(source, native, output)
  from native_logging_transport import check
  report['transport'] = check(source, native, output / 'transport')
  report.update(result='pass', source_binary_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                native_binary_sha256=hashlib.sha256(native.read_bytes()).hexdigest())
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()

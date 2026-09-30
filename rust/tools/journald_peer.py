"""Actual original/native daemons with a controlled child and original IPC collector."""
from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import sys
import time

from openpilot.cereal import log
import openpilot.cereal.messaging as messaging

ROOT = Path(__file__).resolve().parents[2]


class Peer:
  def __init__(self, output: Path, binary: Path | None):
    self.output, self.binary = output, binary
    output.mkdir(parents=True)
    self.prefix = f'journal-qa-{os.getpid()}-{output.parent.name}-{output.name}'
    self.shm = Path('/dev/shm') / ('msgq_' + self.prefix)
    self.shm.mkdir()
    self.trace = output / 'child.jsonl'
    self.sent_ns = time.monotonic_ns()
    self.environment = dict(os.environ, OPENPILOT_PREFIX=self.prefix, HOME=str(output / 'home'),
                            JOURNAL_FIXTURE_TRACE=str(self.trace))
    fixture_bin = output / 'bin'
    fixture_bin.mkdir()
    executable = fixture_bin / 'journalctl'
    executable.write_text('#!/bin/sh\nexec ' + shlex.join([sys.executable, str(ROOT / 'rust/tools/journalctl_fixture.py')]) + ' "$@"\n')
    executable.chmod(0o755)
    self.environment['PATH'] = str(fixture_bin) + ':' + os.environ['PATH']
    self.log_root = output / 'logs'
    self.log_root.mkdir()
    self.process = None
    self.collector = None
    self.child_pid = None
    self.subscribers = {}
    self.records = {topic: [] for topic in ('androidLog', 'logMessage', 'errorLogMessage')}
    self.collector = self.spawn('collector', [sys.executable, str(ROOT / 'rust/tools/logmessaged_reference.py'),
                                             'ipc:///tmp/logmessage' + self.prefix, str(self.log_root)])
    try:
      self.wait(lambda: all((self.shm / topic).exists() for topic in ('logMessage', 'errorLogMessage')))
      command = ([sys.executable, str(ROOT / 'openpilot/system/journald.py')] if binary is None else [str(binary)])
      self.process = self.spawn('journal', command, stdin=subprocess.PIPE)
      self.wait(lambda: (self.shm / 'androidLog').exists() and self.trace.exists())
      started = json.loads(self.trace.read_text().splitlines()[0])
      assert started['event'] == 'started' and started['ppid'] == self.process.pid, started
      self.child_pid = started['pid']
      os.environ['OPENPILOT_PREFIX'] = self.prefix
      self.subscribers = {topic: messaging.sub_sock(topic, timeout=5000) for topic in self.records}
      for subscriber in self.subscribers.values():
        assert subscriber.receive(non_blocking=True) is None
      (output / 'invocation.json').write_text(json.dumps({'argv': command, 'collector': self.collector.args,
        'environment': {key: self.environment[key] for key in ('OPENPILOT_PREFIX', 'HOME', 'PATH', 'JOURNAL_FIXTURE_TRACE')},
        'source_sha256': hashlib.sha256((ROOT / 'openpilot/system/journald.py').read_bytes()).hexdigest(),
        'binary_sha256': None if binary is None else hashlib.sha256(binary.read_bytes()).hexdigest()}, indent=2))
    except BaseException:
      self.close()
      raise

  def spawn(self, name, command, **kwargs):
    with (self.output / (name + '.stdout')).open('wb') as stdout, (self.output / (name + '.stderr')).open('wb') as stderr:
      return subprocess.Popen(command, env=self.environment, cwd=ROOT, stdout=stdout, stderr=stderr, **kwargs)

  def wait(self, predicate):
    until = time.monotonic() + 15
    while not predicate():
      assert self.collector.poll() is None, (self.output / 'collector.stderr').read_text()
      assert self.process is None or self.process.poll() is None, (self.output / 'journal.stderr').read_text()
      assert time.monotonic() < until, self.output
      time.sleep(.01)

  def command(self, value):
    self.process.stdin.write(json.dumps(value).encode() + b'\n')
    self.process.stdin.flush()

  def send(self, payload: bytes):
    with (self.output / 'input.bin').open('ab') as stream:
      stream.write(payload)
    self.sent_ns = time.monotonic_ns()
    self.command({'op': 'write', 'base64': base64.b64encode(payload).decode()})

  def receive(self, topic):
    packet = self.subscribers[topic].receive()
    assert packet is not None, (topic, self.output, self.process.poll(), (self.output / 'journal.stderr').read_text())
    with log.Event.from_bytes(packet) as event:
      assert event.which() == topic
      received_ns = time.monotonic_ns()
      assert self.sent_ns - 1000 <= event.logMonoTime <= received_ns + 1000
      with (self.output / 'timestamps.jsonl').open('a') as stream:
        stream.write(json.dumps({'topic': topic, 'sent_ns': self.sent_ns, 'event_ns': event.logMonoTime,
                                 'received_ns': received_ns}) + '\n')
      value = event.to_dict()
      value.pop('logMonoTime')
    with (self.output / (topic + '.bin')).open('ab') as stream:
      stream.write(packet)
    self.records[topic].append(value)
    return value

  def finish(self, command=None, signum=None, expect_orphan=False):
    started = time.monotonic()
    if command is not None:
      self.command(command)
    if signum is not None:
      self.process.send_signal(signum)
    code = self.process.wait(timeout=5)
    alive = Path(f'/proc/{self.child_pid}').exists()
    trace = [json.loads(line) for line in self.trace.read_text().splitlines()]
    assert alive == expect_orphan, (alive, expect_orphan, trace)
    report = {'exit': code, 'elapsed': time.monotonic() - started, 'child_survived': alive, 'trace': trace}
    (self.output / 'exit.json').write_text(json.dumps(report, indent=2))
    return report

  def close(self):
    if self.process is not None:
      if self.process.poll() is None:
        self.process.kill()
        self.process.wait(timeout=5)
      if self.process.stdin is not None:
        self.process.stdin.close()
    if self.child_pid is not None and Path(f'/proc/{self.child_pid}').exists():
      try:
        os.kill(self.child_pid, signal.SIGTERM)
      except ProcessLookupError:
        pass
    if self.collector is not None:
      self.collector.send_signal(signal.SIGINT)
      self.collector.wait(timeout=5)
    (self.output / 'records.json').write_text(json.dumps(self.records, indent=2))
    self.subscribers.clear()
    shutil.rmtree(self.shm)
    Path('/tmp/logmessage' + self.prefix).unlink(missing_ok=True)

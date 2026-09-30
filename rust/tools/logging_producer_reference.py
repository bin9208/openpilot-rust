"""Original formatter, event and console-policy oracle plus bounded Rust probe I/O."""

from __future__ import annotations
import ast
from contextlib import contextmanager
import io
import json
import logging
import os
from pathlib import Path
import select
import subprocess
from types import SimpleNamespace
from openpilot.common.logging_extra import SwagFormatter, SwagLogger

ROOT = Path(__file__).resolve().parents[2]


class Capture(logging.Handler):
  def __init__(self):
    super().__init__()
    self.records = []

  def emit(self, record):
    self.records.append(record)


def source():
  logger = SwagLogger()
  logger.setLevel(logging.DEBUG)
  capture = Capture()
  logger.addHandler(capture)
  return logger, capture


def special(values):
  fields = dict(values)
  fields.update(nan=float('nan'), positive=float('inf'), negative=float('-inf'), wide_integer=2**127 - 1)
  return fields


def packet(logger, record):
  formatter = SwagFormatter(logger)
  formatter.host = 'test-host'
  for key, value in {
    'pathname': '/source/module.rs',
    'filename': 'module.rs',
    'module': 'module',
    'lineno': 10,
    'funcName': 'run',
    'process': 123,
    'thread': 456,
    'threadName': 'worker',
    'created': 1234.5,
  }.items():
    setattr(record, key, value)
  return bytes([record.levelno]) + formatter.format(record).encode()


def console_handler(value):
  path = ROOT / 'openpilot/common/swaglog.py'
  tree = ast.parse(path.read_text())
  nodes = [
    node
    for node in tree.body
    if (isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id in ('outhandler', 'print_level') for t in node.targets))
    or (isinstance(node, ast.If) and isinstance(node.test, ast.Compare) and isinstance(node.test.left, ast.Name) and node.test.left.id == 'print_level')
  ]
  scope = {'logging': logging, 'os': SimpleNamespace(environ={} if value is None else {'LOGPRINT': value})}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), scope)
  output = io.StringIO()
  scope['outhandler'].setStream(output)
  return scope['outhandler'], output


class Probe:
  def __init__(self, binary: Path, output: Path, endpoint: str | None = None, print_level: str | None = None):
    output.mkdir(parents=True, exist_ok=False)
    self.output = output
    self.stderr = (output / 'stderr.log').open('w')
    env = dict(os.environ)
    if print_level is None:
      env.pop('LOGPRINT', None)
    else:
      env['LOGPRINT'] = print_level
    self.process = subprocess.Popen(
      [binary, *([endpoint] if endpoint else [])], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True, bufsize=1, env=env
    )
    assert self.process.stdin is not None and self.process.stdout is not None
    self.count = 0

  def write(self, command):
    self.process.stdin.write(json.dumps(command, allow_nan=False) + '\n')
    self.process.stdin.flush()

  def receive(self):
    assert select.select([self.process.stdout], [], [], 5)[0], ('probe timeout', self.output)
    line = self.process.stdout.readline()
    assert line, ('probe exited', self.process.poll(), self.output)
    response = json.loads(line)
    with (self.output / 'responses.jsonl').open('a') as stream:
      stream.write(line)
    self.count += 1
    return response

  def command(self, command):
    self.write(command)
    return self.receive()

  def finish(self):
    self.process.stdin.close()
    assert self.process.wait(timeout=5) == 0
    self.stderr.close()

  def close(self):
    if self.process.poll() is None:
      self.process.kill()
      self.process.wait(timeout=5)
    self.stderr.close()


@contextmanager
def probe(*args, **kwargs):
  client = Probe(*args, **kwargs)
  try:
    yield client
  finally:
    client.close()


def original_socket_handler(endpoint, logger):
  import warnings
  import zmq

  path = ROOT / 'openpilot/common/swaglog.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'UnixDomainSocketHandler']
  scope = {'logging': logging, 'os': os, 'warnings': warnings, 'zmq': zmq, 'Paths': SimpleNamespace(swaglog_ipc=lambda: endpoint)}
  exec(compile(tree, str(path), 'exec'), scope)
  return scope['UnixDomainSocketHandler'](SwagFormatter(logger))

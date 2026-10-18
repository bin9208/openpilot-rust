"""Exact final signal source for temporary differential tests, never runtime."""
import ast
import dataclasses
import hashlib
import json
import math
from pathlib import Path

FIXTURE = Path(__file__).resolve().parents[1] / 'upstream/signal-source.json'


def source(path):
  fixture = json.loads(FIXTURE.read_text())
  assert fixture['source_commit'] == '163c000e05641adea88e9221094d45c90c3e186d'
  entry = fixture['files'][path]
  data = entry['content'].encode()
  assert hashlib.sha256(data).hexdigest() == entry['sha256']
  assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == entry['git_blob']
  return entry['content']


def classes(path, names):
  """Retain actual class/function AST, excluding platform-only imports."""
  nodes = [n for n in ast.parse(source(path)).body if isinstance(n, (ast.ClassDef, ast.FunctionDef)) and n.name in names]
  assert {n.name for n in nodes} == set(names)
  scope = {'math': math, 'dataclass': dataclasses.dataclass, 'field': dataclasses.field}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(FIXTURE) + ':' + path, 'exec'), scope)
  return scope


def compare(actual, expected, *, tolerance=0.):
  """Require every discrete value; measure all floating outputs explicitly."""
  import struct
  receipt = {'floats': 0, 'float_bit_differences': 0, 'maximum_absolute_error': 0.}
  def walk(a, e):
    if isinstance(e, dict):
      assert set(a) == set(e)
      for k in e: walk(a[k], e[k])
    elif isinstance(e, (list, tuple)):
      for x, y in zip(a, e, strict=True): walk(x, y)
    elif isinstance(e, float):
      error = abs(a-e)
      receipt['floats'] += 1
      receipt['float_bit_differences'] += int(struct.pack('d', a) != struct.pack('d', e))
      receipt['maximum_absolute_error'] = max(error, receipt['maximum_absolute_error'])
      assert error <= tolerance, (a, e)
      if tolerance == 0.: assert struct.pack('d', a) == struct.pack('d', e), (a, e)
    else: assert a == e, (a, e)
  walk(actual, expected)
  return receipt

#!/usr/bin/env python3
from __future__ import annotations

import argparse
import ast
from functools import cache
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import threading
from types import SimpleNamespace

import numpy as np


class Messages:
  def __init__(self):
    self.raw = b''

  def send(self, service, message):
    assert service == 'rawAudioData'
    assert message.valid and message.rawAudioData.sampleRate == 16000
    self.raw = message.rawAudioData.data

  @staticmethod
  def new_message(service, valid):
    assert service == 'rawAudioData'
    return SimpleNamespace(valid=valid, rawAudioData=SimpleNamespace())


def source():
  path = Path(__file__).resolve().parents[2] / 'openpilot/system/micd.py'
  body = ast.parse(path.read_text())
  names = {'get_a_weighting_filter', 'calculate_spl', 'apply_a_weighting', 'Mic'}
  body.body = [node for node in body.body if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names]
  for node in body.body:
    if isinstance(node, ast.ClassDef):
      node.body = [method for method in node.body if isinstance(method, ast.FunctionDef) and method.name == 'callback']
  namespace = {'np': np, 'cache': cache, 'FFT_SAMPLES': 1600, 'SAMPLE_RATE': 16000, 'REFERENCE_SPL': 2e-5, 'messaging': Messages}
  exec(compile(body, str(path), 'exec'), namespace)
  mic = namespace['Mic']()
  mic.pm = Messages()
  mic.measurements = np.empty(0)
  mic.sound_pressure = mic.sound_pressure_weighted = mic.sound_pressure_level_weighted = 0
  mic.lock = threading.Lock()
  return mic, hashlib.sha256(path.read_bytes()).hexdigest()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  random = np.random.default_rng(126)
  time = np.arange(1600, dtype=np.float64) / 16000
  signal = np.concatenate([
    np.zeros(3200), np.ones(1600) * 0.5,
    *[np.sin(2 * np.pi * frequency * time) * 0.6 for frequency in (10, 100, 1000, 3500, 7990)],
    random.normal(0, 0.4, 1600 * 12), np.ones(1600) * 1e-30,
    np.array([0, -0., 1., -1., 1.1, -1.1, 2., -2., 65536., -65536., np.inf, -np.inf, np.nan] * 130),
    np.zeros(3200),
  ]).astype(np.float32)
  callbacks = []
  offset = 0
  for size in [0, 799, 1, 800, 1601, 17, 4799] * 30:
    part = signal[offset:offset + size]
    callbacks.append(part)
    offset += len(part)
    if offset == len(signal):
      break
  if offset < len(signal):
    callbacks.append(signal[offset:])
  request = {'callbacks': [part.view(np.uint32).tolist() for part in callbacks]}
  result = subprocess.run([str(args.binary)], input=json.dumps(request), capture_output=True, text=True, check=True, timeout=20)
  rows = json.loads(result.stdout)
  assert len(rows) == len(callbacks)
  original, source_hash = source()
  max_error = np.zeros(3)
  snapshots = []
  for part, row in zip(callbacks, rows, strict=True):
    with np.errstate(all='ignore'):
      original.callback(part.reshape(-1, 1), len(part), None, None)
    assert bytes(row['raw']) == original.pm.raw
    assert row['pending'] == original.measurements.size
    expected = np.array([original.sound_pressure, original.sound_pressure_weighted, original.sound_pressure_level_weighted])
    actual = np.array([struct.unpack('<d', struct.pack('<Q', bits))[0] for bits in row['pressure_bits']])
    np.testing.assert_array_equal(np.isfinite(actual), np.isfinite(expected))
    np.testing.assert_array_equal(np.isnan(actual), np.isnan(expected))
    finite = np.isfinite(expected)
    np.testing.assert_allclose(actual[finite], expected[finite], rtol=1e-10, atol=1e-12)
    max_error[finite] = np.maximum(max_error[finite], np.abs(actual[finite] - expected[finite]))
    snapshots.append({'frames': len(part), 'pending': row['pending'], 'raw_sha256': hashlib.sha256(original.pm.raw).hexdigest(), 'pressure': expected.tolist()})
  report = {'source_sha256': source_hash, 'callbacks': len(callbacks), 'samples': len(signal),
            'max_absolute_error': max_error.tolist(), 'raw_bytes': 'exact', 'cases': snapshots}
  (args.output / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
  (args.output / 'native.json').write_text(result.stdout)
  print(json.dumps({key: value for key, value in report.items() if key != 'cases'}))


if __name__ == '__main__':
  main()

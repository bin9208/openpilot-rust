#!/usr/bin/env python3
"""Compare structured logging with original source and real native collector surfaces."""

from __future__ import annotations
import argparse
from collections import Counter
import json
import logging
from pathlib import Path
import numpy as np
from logging_producer_reference import packet, probe, source, special


def records(binary: Path, output: Path) -> dict:
  logger, capture = source()
  counts = Counter()
  random = np.random.default_rng(45123)
  cases = [{}, {'unicode': '한글😀\n\u0000', 'bool': True, 'null': None}, {'nested': {'z': 1, 'a': [2, 3.5]}}]
  cases += [{'finite': float(value), 'negative_zero': -0.0, 'integer': int(index)} for index, value in enumerate(random.uniform(-1e20, 1e20, 200))]
  with probe(binary, output) as client:
    for message in [*cases, 'hello %s', 'line\nsecond', '']:
      for level in (0, 10, 20, 30, 40, 50):
        for exception in (None, 'ValueError: fixture'):
          context = [['local', 'ctx'], ['version', 'rust-test']]
          request = {'action': 'format', 'level': level, 'message': message, 'context': context, 'exception': exception}
          actual = client.command(request)
          record = logging.LogRecord('swaglog', level, 'ignored', 0, message, (), None)
          if exception:
            record.exc_info = (ValueError, ValueError('fixture'), None)
          logger.log_local.ctx = dict(context)
          expected = packet(logger, record)
          assert bytes(actual['packet']) == expected, (request, actual, expected)
          counts['record_packets'] += 1
    for fields in ([], [['debug', False]], [['error', False]], [['debug', None], ['error', None]], [['args', 'overridden']], [['event', 'duplicate']]):
      for arguments in ([], [1, '한글', {'z': False, 'a': None}]):
        for nonfinite in (False, True):
          request = {'action': 'event', 'name': 'producer-event', 'arguments': arguments, 'fields': fields, 'context': [['ctx', 1]], 'special': nonfinite}
          actual = client.command(request)
          logger.log_local.ctx = {'ctx': 1}
          try:
            logger.event('producer-event', *arguments, **(special(fields) if nonfinite else dict(fields)))
          except TypeError:
            assert 'error' in actual
            counts['source_errors'] += 1
          else:
            expected = packet(logger, capture.records[-1])
            assert bytes(actual['packet']) == expected, (request, actual, expected)
            counts['event_packets'] += 1
    client.finish()
  report = {'result': 'pass', **counts, 'comparison': 'exact first byte and ordered Python ASCII JSON; source formatter/event code unchanged'}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def check(binary: Path, fork_binary: Path, collector: Path, output: Path) -> None:
  from logging_producer_native import native, backpressure, console
  from logging_console_failure import check as console_failure
  from logging_producer_transport import forks, collectors, runtime_endpoint

  output.mkdir(parents=True, exist_ok=False)
  report = {
    'records': records(binary, output / 'records'),
    'native': native(binary, output / 'native'),
    'runtime_endpoint': runtime_endpoint(binary, output / 'runtime-endpoint'),
    'backpressure': backpressure(binary, output / 'backpressure'),
    'console': console(binary, output / 'console'),
    'console_failure': console_failure(binary, output / 'console-failure'),
    'fork': forks(fork_binary, output / 'fork'),
    'collectors': collectors(binary, collector, output / 'collectors'),
    'result': 'pass',
  }
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report, indent=2))


if __name__ == '__main__':
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fork-binary', type=Path, required=True)
  parser.add_argument('--collector', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.fork_binary.resolve(), args.collector.resolve(), args.output.resolve())

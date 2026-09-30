#!/usr/bin/env python3
"""Differential timed policy, command, Params, local datetime and logging checks."""
import argparse
import json
import os
from pathlib import Path
import tempfile

from timed_fixtures import environment, native, normalized
from timed_reference import Source


def record_view(records):
  return [{'level': row['levelnum'], 'msg': row['msg'], 'exception': 'exc_info' in row} for row in records]


def compare(binary, output, name, actions, setup=None, zone='UTC', loop=False):
  results = []
  for implementation in ['python', 'rust']:
    with tempfile.TemporaryDirectory(prefix='timed-policy-') as temporary:
      with environment(Path(temporary), zone) as (config, params):
        config['actions'] = actions
        if setup:
          setup(config, params)
        if implementation == 'python':
          source = Source(config, params)
          if loop:
            values = {'service': source.loop(actions), 'publications': source.publications,
                      'sleeps': source.sleeps, 'last_attempt': round(source.last_attempt * 1e9)}
          else:
            values = []
            for action in actions:
              try:
                values.append({'ok': source.action(action)})
              except (OSError, ValueError) as error:
                values.append({'error': type(error).__name__})
          observed = normalized(config, params, source.records())
        else:
          rows, records = native(binary, config, output / (name + '.jsonl'))
          values = [row['result'] for row in rows]
          observed = normalized(config, params, record_view(records))
          if loop:
            assert rows[-1]['last_attempt'] == results[0]['values']['last_attempt'], (name, rows, results[0])
            assert rows[-1]['sleeps'] == results[0]['values']['sleeps'], (name, rows, results[0])
            assert all('ok' in value for value in values), (name, values)
            values = results[0]['values']
        result = {'values': values, **observed}
        results.append(result)
  # Errors cross a language boundary; compare occurrence, then preserve details in artifacts.
  if not loop:
    for left, right in zip(results[0]['values'], results[1]['values'], strict=True):
      assert ('error' in left) == ('error' in right), (name, left, right)
      if 'error' in left:
        left['error'] = right['error'] = '<propagated>'
  assert results[0] == results[1], (name, results)
  (output / (name + '.comparison.json')).write_text(json.dumps(results, indent=2) + '\n')


def step(now, **kwargs):
  gps = {'updated': True, 'has_fix': True, 'log_mono_time': now, 'longitude': 127.0, 'unix_timestamp_millis': 1790000000000}
  gps.update(kwargs)
  return {'kind': 'step', 'monotonic': now, 'gps': gps}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  total = 0

  def case(name, actions, **kwargs):
    nonlocal total
    compare(args.binary, args.output, name, actions, **kwargs)
    total += 1

  longitudes = [-1000, -187.5, -180, -172.5, -37.5, -22.5, -7.500001, -7.5, 0, 7.5, 7.500001, 22.5, 37.5, 127, 202.5, 210, 1000]
  case('gps-banker-rounding', [{'kind': 'gps', 'longitude': value} for value in longitudes])
  def apply(zone, source):
    return {'kind': 'apply', 'zone': zone, 'source': source}
  case('precedence', [apply('bad', 'app'), apply('Etc/GMT-9', 'gps'), apply('Asia/Seoul', 'wifi'),
                      apply('Etc/GMT', 'gps'), apply('America/New_York', 'app'), apply('Asia/Seoul', 'wifi'),
                      apply('America/New_York', 'app'), apply('bad', 'unknown')])
  case('same-zone-promote', [apply('Asia/Seoul', 'gps'), apply('Asia/Seoul', 'wifi'), apply('Asia/Seoul', 'app')])

  def dangling(config, params):
    link = Path(config['paths']['localtime'])
    link.parent.mkdir()
    link.symlink_to(link.parent / 'absent')
  case('dangling-link', [apply('Asia/Seoul', 'wifi')], setup=dangling)
  for command in ['rm', 'ln', 'date']:
    case('command-failure-' + command, [apply('Asia/Seoul', 'wifi')] if command != 'date' else [{'kind': 'set_time', 'epoch': 1790000030.9}],
         setup=lambda config, params, command=command: os.environ.update(TIMED_COMMAND_FAIL=command))
  case('missing-sudo', [{'kind': 'set_time', 'epoch': 1790000030.9}], setup=lambda config, params: (Path(config['params_root']) / 'bin/sudo').unlink())
  case('clock-diff-boundaries', [{'kind': 'set_time', 'epoch': 1790000000 + diff} for diff in [-10.0001, -10, -9.999, 0, 9.999, 10, 10.999]])
  for zone in ['UTC', 'Asia/Seoul', 'America/New_York']:
    for epoch in [1740095999, 1740096000, 1740096001, 2051222399, 2051222400, 2051222401]:
      case(f'wall-{zone.replace("/", "-")}-{epoch}', [{'kind': 'valid'}, {'kind': 'bounds'}], zone=zone,
           setup=lambda config, params, epoch=epoch: config.update(wall=epoch * 1000000000))
    for epoch in [0, 1740096000, 1772951400, 1793500200, 2051222400]:
      def systemd(config, params, epoch=epoch):
        path = Path(config['paths']['systemd'])
        path.touch()
        os.utime(path, (epoch, epoch))
      case(f'systemd-{zone.replace("/", "-")}-{epoch}', [{'kind': 'bounds'}, {'kind': 'valid'}], setup=systemd, zone=zone)
  rows = [step(30_000_000_000, updated=False), step(30_000_000_001, has_fix=False),
          step(60_000_000_001, log_mono_time=58_000_000_000), step(60_000_000_002, log_mono_time=58_000_000_002),
          step(360_000_000_002, unix_timestamp_millis=0), step(360_000_000_003, log_mono_time=400_000_000_000),
          step(400_000_000_000, updated=False), step(401_000_000_000, has_fix=False)]
  case('loop-retries-freshness', rows, loop=True)
  for source in ['app', 'wifi', 'gps', 'unknown']:
    case('loop-source-' + source, [step(301_000_000_000), step(302_000_000_000)], loop=True,
         setup=lambda config, params, source=source: (params / 'TimezoneSource').write_text(source))
  for epoch in [1740095999999, 1740096000000, 1740096000001, 2051222399999, 2051222400000, 2051222400001]:
    case(f'gps-date-{epoch}', [step(10_000_000_000, unix_timestamp_millis=epoch)], loop=True)
  case('loop-ublox-selection', [step(10_000_000_000, updated=False)], loop=True,
       setup=lambda config, params: (params / 'UbloxAvailable').write_text('1'))
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'source_native_scenarios': total}, indent=2) + '\n')
  print(f'PASS: {total} original-source/native scenarios')


if __name__ == '__main__':
  main()

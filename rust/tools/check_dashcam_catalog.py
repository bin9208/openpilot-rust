#!/usr/bin/env python3
import argparse
import ast
import importlib.util
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile
from typing import Any

from aiohttp import web
from check_dashcam_runtime import map_commit_repository


def native(binary, request):
  result = subprocess.run([str(binary.resolve())], input=json.dumps(request), text=True, capture_output=True, check=True)
  return json.loads(result.stdout)


def definitions(path, scope):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef)]
  exec(compile(tree, str(path), 'exec'), scope)


def observed(callback):
  try:
    return {'value': callback()}
  except web.HTTPException as error:
    return {'status': error.status, 'error': error.text}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  feature = root / 'openpilot/selfdrive/carrot/server/features/dashcam'
  report_path = root / 'openpilot/selfdrive/carrot/server/services/dashcam_upload_report.py'
  spec = importlib.util.spec_from_file_location('source_report', report_path)
  report = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(report)
  map_commit_repository(report)
  scope = {
    'os': os,
    'web': web,
    'Any': Any,
    'RLOG_SOURCE_NAMES': ('rlog.zst', 'rlog.bz2', 'rlog'),
    'UPLOAD_SOURCE_GROUPS': (('qcamera', ('qcamera.ts', 'qcamera.mp4')), ('rlog', ('rlog.zst', 'rlog.bz2', 'rlog'))),
  }
  definitions(feature / 'paths.py', scope)
  definitions(feature / 'catalog.py', scope)
  counts = {}

  def check(name, request, expected):
    actual = native(args.binary, request)
    (args.output / f'{name}-input.json').write_text(json.dumps(request, ensure_ascii=False))
    (args.output / f'{name}-source.json').write_text(json.dumps(expected, ensure_ascii=False))
    (args.output / f'{name}-native.json').write_text(json.dumps(actual, ensure_ascii=False))
    for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
      assert left == right, (name, index, left, right)
    counts[name] = len(expected)

  names = ['route--0', ' --1 ', '--0', 'a/--0', 'a\\--0', '', '.', '..', 'route--', 'route--+1', 'route--1_0', 'route---1']
  names += ['route--' + chr(n) for n in range(0x110000) if chr(n).isnumeric()]
  names += ['route--123', 'a--b--001', '\x1croute--1\x1f', 'route--١٢', 'route--00000000000000000000001']
  expected = [{'safe': observed(lambda n=n: scope['safe_segment'](n)), 'index': scope['segment_index'](n), 'route': scope['route_name'](n)} for n in names]
  check('paths', {'mode': 'paths', 'names': names}, expected)

  with tempfile.TemporaryDirectory(prefix='dashcam-source-catalog-') as temp:
    scope['DASHCAM_ROOT'] = temp
    names = []
    variants = [
      [],
      [('qlog.zst', 100)],
      [('rlog', 100)],
      [('rlog.zst', 0), ('rlog', 1024)],
      [('qcamera.mp4', 100), ('qcamera.ts', 1024), ('rlog.zst', 32768), ('rlog', 30)],
      [('rlog.zst', 2048), ('rlog.lock', 0)],
      [('rlog.zst', 99), ('else.lock', 0)],
      [('qcamera.ts', 0), ('qcamera.mp4', 1000), ('rlog.bz2', 100000)],
    ]
    for index, files in enumerate(variants):
      name = f'route--{index}'
      names.append(name)
      directory = Path(temp) / name
      directory.mkdir()
      for filename, size in files:
        with (directory / filename).open('wb') as output:
          output.truncate(size)
    names.append('route--99')
    os.symlink(Path(temp) / 'route--2', Path(temp) / 'route--98')
    names.append('route--98')
    os.symlink(Path(temp) / 'route--2/rlog', Path(temp) / 'route--0/rlog.zst')
    expected = [
      {
        'directory': observed(lambda n=n: scope['segment_dir'](n)),
        'complete': scope['segment_is_complete'](n),
        'files': observed(lambda n=n: scope['segment_file_summary'](str(Path(temp) / n))),
      }
      for n in names
    ]
    check('files', {'mode': 'files', 'names': names, 'root': temp}, expected)

  urls = [
    '',
    'relative/path',
    'HTTP://EXAMPLE.COM/한글 차/%2f%20a?q=x#f',
    'https://x/a%ZZ',
    'https://x/a%ff',
    'https://[bad/path',
    'https://x',
    'https://x/',
    'ftp://x/path',
    'https://x//a|b/../../z',
    'https://x:wrong/a b',
    'https://u:p@host/a?x=1#',
    'https://x/path?',
    'https://x/path#',
  ]
  check('urls', {'mode': 'urls', 'urls': urls}, [report.public_upload_url(url) for url in urls])
  rng = random.Random(61)
  payloads = []
  for length in [0, 1, 2, 3, 10, 24, 25, 40, 100]:
    for scenario in range(4):
      route = '00000001--1234567890'
      items = []
      for index in range(length):
        segment = f'{route}--{index}'
        if scenario == 1 and index % 3 == 0:
          segment = 'bad--' + str(index)
        items.append(
          {
            'ok': scenario != 2 or rng.random() > 0.4,
            'segment': segment,
            'route': route,
            'segmentIndex': index,
            'error': '오류' * (300 if scenario == 3 else 2),
          }
        )
      payloads.append(
        {
          'meta': {'carName': '차', 'dongleId': 'fixture', 'serial': 'none', 'branch': 'dev', 'commit': '1234', 'commitDate': '2026-09-30'},
          'uploadedAt': '2026-09-30 00:00:00',
          'remoteBasePath': 'https://example.test/routes/차 fixture/',
          'results': items,
        }
      )
  check('reports', {'mode': 'reports', 'payloads': payloads}, [{'share': report.upload_share_text(p), 'discord': report.discord_content(p)} for p in payloads])
  (args.output / 'report.json').write_text(json.dumps({'passed': True, **counts}, indent=2) + '\n')
  print(json.dumps(counts))


if __name__ == '__main__':
  main()

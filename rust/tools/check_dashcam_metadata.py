#!/usr/bin/env python3
import argparse
import ast
import base64
from collections.abc import Callable
from contextlib import contextmanager
import copy
import io
import json
import logging
import os
from pathlib import Path
import random
import re
import subprocess
import tempfile
import threading
from typing import Any, Optional

from check_dashcam_runtime import definitions, module


@contextmanager
def environment(values, clear=False):
  saved = dict(os.environ)
  try:
    if clear:
      os.environ.clear()
    os.environ.update(values)
    yield
  finally:
    os.environ.clear()
    os.environ.update(saved)


def source(root, settings_path):
  feature = root / 'openpilot/selfdrive/carrot/server/features/dashcam'
  upload = {'base64': base64, 'os': os, 'subprocess': subprocess, 'Any': Any}
  tree = ast.parse((feature / 'upload.py').read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef)]
  exec(compile(tree, str(feature / 'upload.py'), 'exec'), upload)
  jobs = ast.parse((feature / 'upload_jobs.py').read_text())
  concurrency = next(node for node in ast.walk(jobs) if isinstance(node, ast.Try) and 'CARROT_WEB_UPLOAD_CONCURRENCY' in ast.unparse(node))
  function = ast.parse('def concurrency():\n  pass\n  return concurrency').body[0]
  function.body[0] = concurrency
  exec(compile(ast.fix_missing_locations(ast.Module(body=[function], type_ignores=[])), str(feature / 'upload_jobs.py'), 'exec'), upload)
  transport = module(root / 'openpilot/selfdrive/carrot/web_upload.py', 'reference_metadata_transport')
  services = root / 'openpilot/selfdrive/carrot/server/services'
  capabilities = module(services / 'web_capabilities.py', 'reference_metadata_capabilities')
  settings = {
    '__name__': 'reference_metadata_settings',
    'copy': copy,
    'json': json,
    'logging': logging,
    'os': os,
    're': re,
    'threading': threading,
    'Any': Any,
    'Callable': Callable,
    'Dict': dict,
    'List': list,
    'Optional': Optional,
    'Tuple': tuple,
    'DEFAULT_WEB_UPLOAD_URL': transport.DEFAULT_WEB_UPLOAD_URL,
    'normalize_base_url': transport.normalize_base_url,
    'CARROT_WEB_SETTINGS_PATH': str(settings_path),
    'WEB_DIR': str(root / 'openpilot/selfdrive/carrot/web'),
    'is_known_web_capability': capabilities.is_known_web_capability,
  }
  definitions(services / 'web_settings.py', settings)
  upload['read_web_settings'] = settings['read_web_settings']
  upload['web_upload_settings'] = transport.web_upload_settings
  return upload


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  root = Path(__file__).resolve().parents[2]
  requests, expected = [], []
  rng = random.Random(61)
  with tempfile.TemporaryDirectory(prefix='dashcam-metadata-') as temp:
    settings_path = Path(temp) / 'settings.json'
    original = source(root, settings_path)
    hardware_path = root / 'openpilot/system/hardware/base.py'
    hardware_class = next(node for node in ast.parse(hardware_path.read_text()).body if isinstance(node, ast.ClassDef) and node.name == 'HardwareBase')
    cmdline_method = next(node for node in hardware_class.body if isinstance(node, ast.FunctionDef) and node.name == 'get_cmdline')
    for cmdline in [
      '',
      'androidboot.serialno=first',
      'a=1 androidboot.serialno=first androidboot.serialno=second',
      'androidboot.serialno=bad=tail',
      'a=1\tandroidboot.serialno=hidden',
      'androidboot.serialno=first\tandroidboot.serialno=second',
      'androidboot.serialno=',
      ' androidboot.serialno= last',
      'androidboot.serialno=value\n',
      'androidboot.serialno=a==b androidboot.serialno=correct',
    ]:
      hardware_scope = {'open': lambda path, data=cmdline: io.StringIO(data)}
      exec(compile(ast.Module(body=[cmdline_method], type_ignores=[]), str(hardware_path), 'exec'), hardware_scope)
      expected.append(hardware_scope['get_cmdline']().get('androidboot.serialno'))
      requests.append({'op': 'serial', 'cmdline': cmdline})
    values = [None, '', ' ', '0', '-8', '6', '7', '+4', '1_0', '٠٠٣', '４', '²', '_3', '3_', '3__0', '\x1c3\x1f', '\u00853\u0085', '9' * 200, '9' * 4400]
    values += [''.join(rng.choices('+-_0123456789 ٠٢٤\t', k=rng.randrange(1, 24))) for _ in range(600)]
    for value in values:
      with environment({} if value is None else {'CARROT_WEB_UPLOAD_CONCURRENCY': value}, clear=True):
        expected.append(original['concurrency']())
      requests.append({'op': 'concurrency', 'value': value})
    for _ in range(500):
      key = rng.choice(['k', 'fixture-key', '한글', ''])
      raw = bytes(rng.randrange(256) for _ in range(rng.randrange(0, 40)))
      token = base64.urlsafe_b64encode(raw).decode().rstrip('=')
      for value in [token, token + '===', token[:2] + '!\n' + token[2:], '\x1c' + token + '\x1f', token.replace('-', '+').replace('_', '/')]:
        requests.append({'op': 'decode', 'value': value, 'key': key})
        expected.append(original['decode_obfuscated'](value, key))
    for index, raw in enumerate(
      [
        {},
        [],
        None,
        {'toss_upload_url': 'http://fixture/path/'},
        {'web_upload_url': '', 'toss_upload_url': 'http://ignored'},
        *[
          {'web_upload_url': value}
          for value in [
            'https://op.wjcloud.kr/',
            'https://shind0.synology.me',
            'HTTP://fixture',
            'ftp://fixture',
            'http://fixture/path/',
            '\x1chttps://fixture\x1f',
            42,
            True,
            {},
            None,
          ]
        ],
      ]
    ):
      path = Path(temp) / f'settings-{index}.json'
      path.write_text(json.dumps(raw))
      for env in [{}, {'CARROT_WEB_UPLOAD_URL': ' http://override/// ', 'CARROT_WEB_UPLOAD_TOKEN': ' fixture-token '}, {'CARROT_WEB_UPLOAD_URL': 'invalid'}]:
        settings_path.write_text(json.dumps(raw))
        with environment(env, clear=True):
          try:
            result = {'value': list(original['upload_target_settings']())}
          except ValueError as error:
            result = {'error': str(error)}
        requests.append(
          {
            'op': 'target',
            'path': str(path),
            'environment': {'upload_url': env.get('CARROT_WEB_UPLOAD_URL', ''), 'upload_token': env.get('CARROT_WEB_UPLOAD_TOKEN', '')},
          }
        )
        expected.append(result)
    binary_repo = Path(temp) / 'binary-git'
    subprocess.run(['git', 'init', '--quiet', str(binary_repo)], check=True)
    digest = (
      subprocess.run(['git', 'hash-object', '-w', '--stdin'], cwd=binary_repo, input=b'\xffinvalid\n', capture_output=True, check=True).stdout.decode().strip()
    )
    git_args = ['cat-file', 'blob', digest]
    with environment({'CARROT_REPO_DIR': str(binary_repo)}):
      expected.append(original['git_text'](git_args, 'unknown'))
    requests.append({'op': 'git', 'repo': str(binary_repo), 'args': git_args, 'default': 'unknown'})
    for repo in [root, Path(temp), Path(temp) / 'missing']:
      for git_args in [
        ['branch', '--show-current'],
        ['rev-parse', '--short', 'HEAD'],
        ['show', '-s', '--date=format:%Y-%m-%d %H:%M:%S', '--format=%cd', 'HEAD'],
      ]:
        with environment({'CARROT_REPO_DIR': str(repo)}):
          expected.append(original['git_text'](git_args, 'unknown'))
        requests.append({'op': 'git', 'repo': str(repo), 'args': git_args, 'default': 'unknown'})
    completed = subprocess.run([str(args.binary.resolve())], input=json.dumps(requests), text=True, capture_output=True, check=True)
  actual = json.loads(completed.stdout)
  assert len(actual) == len(expected)
  mismatches = [
    {'index': i, 'request': request, 'expected': want, 'actual': got}
    for i, (request, want, got) in enumerate(zip(requests, expected, actual, strict=True))
    if want != got
  ]
  report = {'passed': not mismatches, 'cases': len(requests), 'mismatches': mismatches}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'cases': len(requests), 'mismatches': len(mismatches)}))
  assert not mismatches, mismatches[:3]


if __name__ == '__main__':
  main()

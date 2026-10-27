#!/usr/bin/env python3
"""Retain the original PR event and hash-verifiable checkout commit as data."""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys

REPO = 'bin9208/openpilot-rust'
MAX_COMMIT = 64*1024
FIELDS = {'schema', 'repository', 'repository_id', 'head_repository_id', 'base_repository_id',
          'run_id', 'run_attempt', 'job', 'event', 'runner_name', 'runner_environment', 'ref',
          'pr_number', 'head_sha', 'base_ref', 'base_sha', 'checkout_sha', 'checkout_tree', 'commit_object_base64'}


def positive(value):
  return type(value) is int and value > 0


def sha(value):
  return isinstance(value, str) and re.fullmatch('[0-9a-f]{40}', value) is not None


def commit_object(raw):
  if not raw or len(raw) > MAX_COMMIT or b'\n\n' not in raw:
    raise ValueError('validation commit object is missing or exceeds its bound')
  headers = raw.split(b'\n\n', 1)[0].splitlines()
  trees = [line[5:].decode('ascii') for line in headers if line.startswith(b'tree ')]
  parents = [line[7:].decode('ascii') for line in headers if line.startswith(b'parent ')]
  if len(trees) != 1 or len(parents) != 2 or not all(sha(value) for value in [*trees, *parents]):
    raise ValueError('validation requires an actual two-parent PR merge commit')
  digest = hashlib.sha1(b'commit '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
  return digest, trees[0], parents


def verify(value, run_id, attempt, job, repository_id, source, pr_number, target_base='dev'):
  if (set(value) != FIELDS or type(value['schema']) is not int or value['schema'] != 1
      or value['repository'] != REPO or value['event'] != 'pull_request'
      or value['runner_name'] != 'bin-actions' or value['runner_environment'] != 'self-hosted'
      or target_base not in ('dev', 'main') or value['job'] != job or value['base_ref'] != target_base
      or value['ref'] != f'refs/pull/{pr_number}/merge'
      or value['head_sha'] != source or not sha(value['base_sha'])
      or not sha(value['checkout_sha']) or not sha(value['checkout_tree'])):
    raise ValueError('validation context is not the requested same-repository PR event')
  for field, expected in [('run_id', run_id), ('run_attempt', attempt), ('pr_number', pr_number),
                          ('repository_id', repository_id), ('head_repository_id', repository_id),
                          ('base_repository_id', repository_id)]:
    if not positive(value[field]) or value[field] != expected:
      raise ValueError('validation context run/attempt/PR/repository identity differs')
  encoded = value['commit_object_base64']
  if not isinstance(encoded, str) or len(encoded) > 4*((MAX_COMMIT+2)//3):
    raise ValueError('validation commit encoding exceeds its bound')
  raw = base64.b64decode(encoded, validate=True)
  digest, tree, parents = commit_object(raw)
  if digest != value['checkout_sha'] or tree != value['checkout_tree'] or parents != [value['base_sha'], source]:
    raise ValueError('validation commit bytes do not bind the recorded base and source head')
  return value['pr_number'], value['base_sha']


def receipt(event, env, raw):
  pull = event['pull_request']
  repositories = (event['repository'], pull['base']['repo'], pull['head']['repo'])
  if any(repo['full_name'] != REPO for repo in repositories):
    raise ValueError('validation event belongs to a different repository')
  value = dict(schema=1, repository=env['GITHUB_REPOSITORY'], repository_id=int(env['GITHUB_REPOSITORY_ID']),
      head_repository_id=pull['head']['repo']['id'], base_repository_id=pull['base']['repo']['id'],
      run_id=int(env['GITHUB_RUN_ID']), run_attempt=int(env['GITHUB_RUN_ATTEMPT']), job=env['GITHUB_JOB'],
      event=env['GITHUB_EVENT_NAME'], runner_name=env['RUNNER_NAME'], runner_environment=env['RUNNER_ENVIRONMENT'],
      ref=env['GITHUB_REF'], pr_number=event['number'], head_sha=pull['head']['sha'],
      base_ref=pull['base']['ref'], base_sha=pull['base']['sha'], checkout_sha=env['GITHUB_SHA'],
      checkout_tree=commit_object(raw)[1], commit_object_base64=base64.b64encode(raw).decode('ascii'))
  if not positive(event['repository']['id']) or event['repository']['id'] != value['repository_id']:
    raise ValueError('validation event repository ID differs from the runner context')
  verify(value, value['run_id'], value['run_attempt'], value['job'], value['repository_id'],
      value['head_sha'], value['pr_number'], target_base=value['base_ref'])
  return value


def pairs(items):
  result = {}
  for key, value in items:
    if key in result:
      raise ValueError('duplicate validation event JSON key')
    result[key] = value
  return result


def retain(job, data, verify_only=False):
  path = job/'validation-context.json'
  if verify_only:
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
      info = os.fstat(stream.fileno())
      if (not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) != 0o444
          or info.st_uid != os.geteuid() or stream.read(256*1024+1) != data):
        raise ValueError('original validation context changed during the job')
    return
  if shutil.disk_usage(job).free < len(data)+65536:
    raise ValueError('insufficient capacity for this small validation receipt')
  fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o444)
  with os.fdopen(fd, 'wb') as stream:
    os.fchmod(stream.fileno(), 0o444)
    stream.write(data)
    stream.flush()
    os.fsync(stream.fileno())


def main():
  if (os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('RUNNER_ENVIRONMENT') != 'self-hosted'
      or os.environ.get('RUNNER_NAME') != 'bin-actions' or os.environ.get('GITHUB_REPOSITORY') != REPO
      or os.environ.get('GITHUB_EVENT_NAME') != 'pull_request' or os.geteuid() == 0
      or not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode)):
    raise ValueError('validation context requires the registered non-root PR Actions job and isolated Python')
  if sys.argv[1:] not in ([], ['--verify']):
    raise ValueError('validation context accepts only an optional --verify')
  job = Path(os.environ['CI_JOB_TEMP'])
  owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  if (job.parent != Path('/tmp') or not re.fullmatch(r'openpilot-ci\.[A-Za-z0-9]+', job.name)
      or job.is_symlink() or job.resolve() != job or job.stat().st_uid != os.geteuid()
      or (job/'owner').is_symlink() or (job/'owner').read_text() != owner):
    raise ValueError('validation context output is not owned by this job')
  fd = os.open(os.environ['GITHUB_EVENT_PATH'], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
  with os.fdopen(fd, 'rb') as stream:
    if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
      raise ValueError('validation event is not a regular runner file')
    raw_event = stream.read(2*1024**2+1)
  if len(raw_event) > 2*1024**2:
    raise ValueError('validation event exceeds its input bound')
  event = json.loads(raw_event, object_pairs_hook=pairs)
  root = Path(os.environ['GITHUB_WORKSPACE'])
  command = ['/usr/bin/git', '--no-replace-objects', '-C', str(root)]
  environment = dict(PATH='/usr/bin:/bin', LC_ALL='C', GIT_OPTIONAL_LOCKS='0',
      GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_SYSTEM='/dev/null')
  head = subprocess.check_output([*command, 'rev-parse', 'HEAD'], env=environment, timeout=30).decode().strip()
  if head != os.environ['GITHUB_SHA']:
    raise ValueError('validation checkout differs from the original workflow merge SHA')
  # Raw headers preserve parents even with Actions' shallow fetch-depth=1.
  raw = subprocess.check_output([*command, 'cat-file', 'commit', head], env=environment, timeout=30)
  value = receipt(event, os.environ, raw)
  data = (json.dumps(value, sort_keys=True, indent=2)+'\n').encode()
  retain(job, data, verify_only=sys.argv[1:] == ['--verify'])
  print(json.dumps({key: value[key] for key in ('run_id', 'run_attempt', 'job', 'pr_number', 'head_sha', 'base_sha', 'checkout_sha')}))


if __name__ == '__main__':
  try:
    main()
  except (ValueError, TypeError, KeyError, OSError, subprocess.SubprocessError) as error:
    print('validation context: '+str(error), file=sys.stderr)
    sys.exit(1)

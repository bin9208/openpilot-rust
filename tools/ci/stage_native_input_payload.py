#!/usr/bin/env python3
"""Retain only native input files and evidence, without copying the debug tree."""
import hashlib
import json
import os
import runpy
from pathlib import Path
import shutil
import stat

PREFIX = 'native-agnos/runtime-inputs-evidence/'
PROOFS = [PREFIX+'native-bootstrap-inputs.candidate.json', PREFIX+'publication-candidate.json',
          PREFIX+'summary.json', 'native-agnos-environment-599.json']


def stage(job, source, run_id, attempt):
  manifest = json.loads((job/(PREFIX+'publication-candidate.json')).read_text())
  if (manifest['workflow_source_commit'] != source or manifest['source_run_id'] != run_id
      or manifest['source_run_attempt'] != attempt or manifest['source_job'] != 'compiler'
      or manifest['published_release'] is not False or manifest['committed_input_index'] is not False
      or manifest['source_index_resolver_cases_passed'] != ['git-index-initial', 'git-index-reused', 'git-index-docs']
      or len(manifest['files']) != 34):
    raise ValueError('native payload requires this completed producer identity and cases')
  files = {}
  for row in manifest['files']:
    path = Path(row['path'])
    if (not path.is_absolute() or path.resolve(strict=True) != path or not path.is_relative_to(job)
        or path.is_symlink() or not stat.S_ISREG(path.stat().st_mode) or str(path) in files):
      raise ValueError('native payload member is outside the owned producer files')
    with path.open('rb') as stream:
      digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    if (type(row['bytes']) is not int or path.stat().st_size != row['bytes'] or row['bytes'] <= 0
        or digest != row['sha256']):
      raise ValueError('native payload member differs from its exact SHA/bytes')
    files[str(path)] = path
  for name in PROOFS:
    path = job/name
    if (path.is_symlink() or path.resolve(strict=True) != path or not path.is_file()
        or path.stat().st_size > 16*1024**2):
      raise ValueError('native payload proof is absent, redirected or too large')
    files[str(path)] = path
  logical = sum(path.stat().st_size for path in files.values())
  directories = {parent for path in files.values() for parent in path.relative_to(job).parents}
  expected_growth = (len(directories)+len(files)+1)*4096+65536
  if logical > 4*1024**3+64*1024**2 or shutil.disk_usage(job).free < expected_growth+64*1024**2:
    raise ValueError('insufficient capacity for this native input payload; other data retained')
  root = job/'native-input-payload'
  root.mkdir()
  for path in files.values():
    destination = root/path.relative_to(job)
    destination.parent.mkdir(parents=True, exist_ok=True)
    os.link(path, destination, follow_symlinks=False)
  return dict(files=len(files), logical_bytes=logical, copied_bytes=0, source_commit=source,
      source_run_id=run_id, source_run_attempt=attempt, expected_metadata_growth_bytes=expected_growth, staged=str(root))


def main():
  runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_environment'](self_hosted_only=True)
  if os.environ.get('GITHUB_REPOSITORY') != 'bin9208/openpilot-rust':
    raise ValueError('native input payload staging requires the registered Actions context')
  job = Path(os.environ['CI_JOB_TEMP'])
  owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.') or job.is_symlink()
      or job.resolve() != job or job.stat().st_uid != os.geteuid() or (job/'owner').read_text() != owner):
    raise ValueError('native input payload job identity differs')
  print(json.dumps(stage(job, os.environ['GITHUB_SHA'], int(os.environ['GITHUB_RUN_ID']), int(os.environ['GITHUB_RUN_ATTEMPT'])), indent=2))


if __name__ == '__main__':
  main()

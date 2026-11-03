#!/usr/bin/env python3
"""Retain only native input files and evidence, without copying the debug tree."""
import hashlib
import importlib.util
import json
import os
import re
import runpy
from pathlib import Path
import shutil
import stat
from urllib.parse import urlsplit

PREFIX = 'native-agnos/runtime-inputs-evidence/'
PROOFS = [PREFIX+'native-bootstrap-inputs.candidate.json', PREFIX+'publication-candidate.json',
          PREFIX+'summary.json', 'native-agnos-environment-599.json']


def git_inputs(job, payload, source):
  """Retain inputs actually consumed by the existing production preparer."""
  work = job/'native-agnos'
  context = json.loads((work/'native-bootstrap-context.json').read_text())
  initial = json.loads((job/(PREFIX+'initial.json')).read_text())
  if context['source_commit'] != source or initial['runtime_inputs']['source']['commit'] != source:
    raise ValueError('installed inputs require the current completed production source')
  guest = Path(context['guest_work'])
  cache = work/'bootstrap-current-cache'
  aliases = {
      'rust-assets': 'native-rust-assets', 'cargo-assets': 'native-cargo-assets',
      'python-assets': 'native-python-assets', 'capnp-assets': 'native-capnp-assets',
      'raylib-sdk-assets': 'native-raylib-assets', 'ffmpeg-headers-assets': 'native-ffmpeg-source',
      'opencv-headers-assets': 'native-opencv-source', 'model-wheel-assets': 'native-model-wheel-assets',
      'model-assets': 'native-model-assets', 'source-lfs-assets': 'native-source-lfs-assets',
      'usbgpu-model-assets': 'native-usbgpu-model-assets',
  }
  targets = {name: job/path for name, path in aliases.items()}
  targets.update({name: cache/name for name in ('codecs-assets', 'component-assets')})
  files, categories = {}, set()
  folder = work/Path(initial['runtime_inputs']['native_configuration']).relative_to(guest).parent
  operations = json.loads((folder/'operations.json').read_text())
  expanded_lfs = {}
  if 'source-lfs' in operations:
    lfs = operations['source-lfs']
    if not re.fullmatch('[0-9a-f]{64}', lfs['identity']):
      raise ValueError('installed LFS generation has no content identity')
    root = guest/'bootstrap-current-cache/source-lfs-prefixes'/lfs['identity']/'generation/tree'
    if Path(lfs['root']) != root or lfs['source']['commit'] != source:
      raise ValueError('installed LFS generation differs from the consumed source')
    for name, asset in lfs['assets'].items():
      relative, path = Path(name), Path(asset['path'])
      if relative.is_absolute() or '..' in relative.parts or path != root/relative:
        raise ValueError('installed LFS member escaped its consumed generation')
      expanded_lfs[path] = (asset['sha256'], asset['bytes'])

  def retain(path, target, checksum, size):
    if (not path.is_relative_to(job) or path.resolve(strict=True) != path or path.is_symlink()
        or not stat.S_ISREG(path.stat().st_mode) or path.stat().st_size != size
        or type(size) is not int or size <= 0):
      raise ValueError('installed input is not a bounded owned producer file')
    with path.open('rb') as stream:
      if hashlib.file_digest(stream, 'sha256').hexdigest() != checksum:
        raise ValueError('installed input differs from its consumed receipt')
    value = dict(path=path, sha256=checksum, bytes=size)
    if target in files and files[target] != value:
      raise ValueError('conflicting installed input destinations')
    files[target] = value

  def consumed(value):
    if isinstance(value, list):
      for child in value:
        consumed(child)
    elif isinstance(value, dict):
      if {'path', 'sha256', 'bytes', 'transferred_bytes', 'stored_bytes'} <= value.keys():
        path = Path(value['path'])
        suffix = Path('sha256')/value['sha256']/'blob'
        matches = [name for name in targets if path == guest/'bootstrap-current-cache'/name/suffix
                   or path == targets[name]/suffix]
        if not matches and expanded_lfs.get(path) == (value['sha256'], value['bytes']):
          # Source-LFS preparation reports its expanded output. Retain the
          # independently hashed raw cache blob used to produce that output.
          matches = ['source-lfs-assets']
        if len(matches) != 1:
          raise ValueError('consumed asset is outside its existing cache')
        name = matches[0]
        retain(targets[name]/suffix, f'third_party/native_inputs/assets/{name}/{value["sha256"]}',
               value['sha256'], value['bytes'])
        categories.add(name)
      for child in value.values():
        consumed(child)

  for report in (initial, operations,
                 json.loads((job/'native-model-wheels-initial.json').read_text())):
    consumed(report)
  if categories != set(targets):
    raise ValueError('installed inputs omitted a production asset category')
  index_path = job/(PREFIX+'native-bootstrap-inputs.candidate.json')
  index = json.loads(index_path.read_text())
  manifest = json.loads((job/(PREFIX+'publication-candidate.json')).read_text())
  members = {row['sha256']: row for row in manifest['files']}
  for pin in [index['seed'], index['system_sdk'], *index['providers'].values()]:
    row = members[pin['sha256']]
    retain(Path(row['path']), f'third_party/native_inputs/metadata/{pin["sha256"]}.json',
           pin['sha256'], pin['bytes'])
  seed = json.loads(Path(members[index['seed']['sha256']]['path']).read_text())
  for pin in seed['assets'].values():
    row = members[pin['sha256']]
    name = Path(urlsplit(pin['url']).path).name
    retain(Path(row['path']), f'third_party/native_inputs/seeds/{index["seed"]["sha256"]}/{name}',
           pin['sha256'], pin['bytes'])
  retain(index_path, 'rust/native-bootstrap-inputs.json',
         hashlib.sha256(index_path.read_bytes()).hexdigest(), index_path.stat().st_size)
  spec = importlib.util.spec_from_file_location('existing_input_chunks',
      Path(__file__).resolve().parents[2]/'openpilot/common/file_chunker.py')
  chunks = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(chunks)
  large = sum(row['bytes'] for name, row in files.items()
              if '/assets/' in name and row['bytes'] > chunks.CHUNK_SIZE)
  growth = 2*large+(len(files)+64)*8192
  if shutil.disk_usage(job).free < growth+64*1024**2:
    raise ValueError('insufficient capacity to retain installed Git inputs')
  root = payload/'installed-source'
  root.mkdir()
  for name, row in files.items():
    target = root/name
    target.parent.mkdir(parents=True, exist_ok=True)
    if '/assets/' in name and row['bytes'] > chunks.CHUNK_SIZE:
      shutil.copyfile(row['path'], target)
      chunks.chunk_file(target, chunks.get_chunk_targets(target, row['bytes']))
    else:
      os.link(row['path'], target, follow_symlinks=False)
  retained = {}
  for path in root.rglob('*'):
    if path.is_file():
      with path.open('rb') as stream:
        retained[path.relative_to(root).as_posix()] = dict(bytes=path.stat().st_size,
            sha256=hashlib.file_digest(stream, 'sha256').hexdigest())
  report = dict(schema=1, source_commit=source, index_sha256=hashlib.sha256(index_path.read_bytes()).hexdigest(),
                files=retained, consumed_asset_categories=sorted(categories), project_runtime_binaries_included=False)
  (payload/'installed-source.json').write_text(json.dumps(report, indent=2)+'\n')
  return dict(files=len(retained), consumed_asset_categories=sorted(categories), expected_growth_bytes=growth)


def stage(job, source, run_id, attempt):
  manifest = json.loads((job/(PREFIX+'publication-candidate.json')).read_text())
  cases = manifest['source_index_resolver_cases_passed']
  environment = json.loads((job/'native-agnos-environment-599.json').read_text())
  if (cases != ['git-index-initial', 'git-index-reused', 'git-index-docs']
      and not (environment.get('validation_scope') == 'inputs' and cases == ['git-index-initial'])):
    raise ValueError('native payload lacks its selected input preparation cases')
  if (manifest['workflow_source_commit'] != source or manifest['source_run_id'] != run_id
      or manifest['source_run_attempt'] != attempt or manifest['source_job'] != 'compiler'
      or manifest['published_release'] is not False or manifest['committed_input_index'] is not False
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
  runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_github_native_arm']()
  if os.environ.get('GITHUB_REPOSITORY') != 'bin9208/openpilot-rust':
    raise ValueError('native input payload staging requires the registered Actions context')
  job = Path(os.environ['CI_JOB_TEMP'])
  owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.') or job.is_symlink()
      or job.resolve() != job or job.stat().st_uid != os.geteuid() or (job/'owner').read_text() != owner):
    raise ValueError('native input payload job identity differs')
  result = stage(job, os.environ['GITHUB_SHA'], int(os.environ['GITHUB_RUN_ID']), int(os.environ['GITHUB_RUN_ATTEMPT']))
  result['installed_git_inputs'] = git_inputs(job, Path(result['staged']), os.environ['GITHUB_SHA'])
  print(json.dumps(result, indent=2))


if __name__ == '__main__':
  main()

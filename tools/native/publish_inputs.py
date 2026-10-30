#!/usr/bin/env python3
"""Validate a successful native CI artifact and optionally publish its inputs.

Actions-only operation. Binary files are hashed and uploaded, never executed.
The resulting index is retained for a separate reviewed Git change.
"""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import resource
import select
import shutil
import signal
import stat
import struct
import subprocess
import sys
import time
from urllib.parse import quote, unquote, urlsplit
import zipfile

REPO = 'bin9208/openpilot-rust'
BASE = f'https://github.com/{REPO}/releases/download/'
PRODUCER = '.github/workflows/runtime-native-environment.yml'
ROLES = ('ffmpeg', 'raylib', 'opencv', 'numerics', 'geos', 'brotli', 'acados', 'sdl',
         'turbojpeg', 'ffmpeg-tools', 'freesans', 'panda', 'mpc', 'dbc', 'usbgpu')
CASES = ['git-index-initial', 'git-index-reused', 'git-index-docs']
PREFIX = 'native-agnos/runtime-inputs-evidence/'
MARKER = 'openpilot-native-inputs-publication-v1'
FOUNDATION_ARM_JOB = 'Actual offline Cargo reuse (ARM native)'
GATES = [
    ('runtime-source-build.yml', ('push', 'workflow_dispatch'),
      {FOUNDATION_ARM_JOB, 'Actual offline Cargo reuse (X64)'}),
    ('runtime-native-environment.yml', ('push', 'workflow_dispatch'),
      {'Native compiler in pinned AGNOS ARM userspace'}),
    ('native-resource-guard.yml', ('push', 'workflow_dispatch'), {'Real memory limit and cancellation (X64 host kernel)'}),
    ('rust.yml', ('pull_request',), {'rust checks', 'rust workspace checks', 'rust aarch64 build'}),
    ('integration.yml', ('pull_request',), {'integration gate', 'build / build release'}),
    ('user-docs.yaml', ('pull_request',), {'check mapped user docs'}),
]
PROOF_JOBS = {'rust.yml': 'workspace', 'integration.yml': 'build_release',
              'user-docs.yaml': 'check_user_docs'}
PROOF_JOB_NAMES = {'workspace': 'rust workspace checks', 'build_release': 'build / build release',
                   'check_user_docs': 'check mapped user docs'}
SELF_HOSTED_GATES = {'runtime-source-build.yml', 'runtime-native-environment.yml', 'native-resource-guard.yml'}
_CONTEXT = importlib.util.spec_from_file_location('native_validation_context_source',
    Path(__file__).resolve().parents[1]/'ci/validation_context.py')
validation_context = importlib.util.module_from_spec(_CONTEXT)
# Compile only this checkout's helper source; an ignored timestamp pyc is never
# evidence of the publisher's current code. Artifact commit bytes remain data.
exec(compile(_CONTEXT.loader.get_source(_CONTEXT.name), _CONTEXT.origin, 'exec'), validation_context.__dict__)
runner_policy = validation_context.runner_policy


def pairs(items):
  result = {}
  for key, value in items:
    if key in result:
      raise ValueError('duplicate publication JSON key')
    result[key] = value
  return result


def decode(data):
  return json.loads(data, object_pairs_hook=pairs)


def checksum(value, length=64):
  return isinstance(value, str) and re.fullmatch('[0-9a-f]{'+str(length)+'}', value) is not None


def positive(value):
  return type(value) is int and value > 0


def asset(value, maximum=2*1024**3):
  if (set(value) != {'url', 'sha256', 'bytes'} or not checksum(value['sha256'])
      or not positive(value['bytes']) or value['bytes'] > maximum):
    raise ValueError('invalid native publication asset')
  url = urlsplit(value['url'])
  if (url.scheme != 'https' or url.netloc != 'github.com' or url.query or url.fragment
      or not value['url'].startswith(BASE)):
    raise ValueError('publication URL must belong to this repository release')
  parts = url.path.split('/')
  if len(parts) != 7 or any(unquote(p) != p for p in parts):
    raise ValueError('ambiguous native release URL')
  tag, name = parts[-2:]
  if (not re.fullmatch(r'native-[A-Za-z0-9][A-Za-z0-9._-]{0,190}', tag)
      or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,190}', name)
      or '..' in tag or tag.endswith(('.', '.lock')) or name in ('.', '..')):
    raise ValueError('unsafe native release tag or filename')
  return tag, name


def room(root, growth):
  if growth < 0 or shutil.disk_usage(root).free < growth+64*1024**2:
    raise ValueError('insufficient space for this publication operation; other data retained')


def transfer(command, expected, destination=None):
  """Stream a fixed byte count with bounded memory; cancel only our own child."""
  digest, count = hashlib.sha256(), 0
  import tempfile
  with tempfile.TemporaryFile() as errors:
    output = destination.open('xb') if destination is not None else None
    child = None
    try:
      environment = dict(os.environ, GH_PROMPT_DISABLED='1')
      environment.pop('GH_DEBUG', None)
      child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors, start_new_session=True, env=environment)
      deadline = time.monotonic()+1200
      while True:
        if time.monotonic() > deadline:
          raise ValueError('native asset transfer observation timed out')
        if not select.select([child.stdout], [], [], 1)[0]:
          continue
        chunk = os.read(child.stdout.fileno(), 65536)
        if not chunk:
          break
        count += len(chunk)
        if count > expected['bytes']:
          raise ValueError('native transfer exceeded its independently pinned size')
        digest.update(chunk)
        if output is not None:
          output.write(chunk)
      if child.wait(timeout=10) != 0:
        errors.seek(0, os.SEEK_END)
        errors.seek(max(0, errors.tell()-4096))
        raise ValueError(f'native asset transfer failed ({child.returncode}): '+errors.read().decode(errors='replace'))
      if count != expected['bytes'] or digest.hexdigest() != expected['sha256']:
        raise ValueError('native transfer differs from its independent size/SHA pin')
      if output is not None:
        output.flush()
        os.fsync(output.fileno())
      return count
    finally:
      if child is not None:
        child.stdout.close()
      if child is not None and child.poll() is None:
        os.killpg(child.pid, signal.SIGTERM)
        try:
          child.wait(timeout=3)
        except subprocess.TimeoutExpired:
          os.killpg(child.pid, signal.SIGKILL)
          child.wait(timeout=3)
      if output is not None:
        output.close()


class GitHub:
  def __init__(self, root=None):
    self.root = root

  def json(self, method, route, value=None, missing=False):
    if route.startswith('/') or '..' in route.split('/'):
      raise ValueError('invalid repository API route')
    command = ['gh', 'api', '--hostname', 'github.com', '--include', '--method', method,
        f'repos/{REPO}/'+route, '-H', 'Accept: application/vnd.github+json',
        '-H', 'X-GitHub-Api-Version: 2026-03-10']
    if value is not None:
      command += ['--input', '-']
    for attempt in range(3):
      response = subprocess.run(command, input=None if value is None else json.dumps(value).encode(),
          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90, check=False)
      if len(response.stdout) > 16*1024**2:
        raise ValueError('GitHub metadata response exceeded its bound')
      parts = re.split(rb'\r?\n\r?\n', response.stdout, maxsplit=1)
      if len(parts) != 2 or not re.match(rb'HTTP/\S+ \d{3}', parts[0]):
        if method == 'GET' and attempt < 2:
          time.sleep(2**attempt)
          continue
        raise ValueError('GitHub response lacked its bounded HTTP metadata')
      headers, body = parts
      status = int(headers.splitlines()[0].split()[1])
      if method == 'GET' and status in (429, 500, 502, 503, 504) and attempt < 2:
        time.sleep(2**attempt)
        continue
      if status == 404 and missing:
        return None
      if response.returncode or not 200 <= status < 300:
        raise ValueError(f'GitHub repository API failed: {method} {route} HTTP{status}')
      return decode(body) if body.strip() else {}
    raise ValueError('GitHub metadata retries exhausted')

  def collect(self, route, key=None):
    result = []
    for page in range(1, 26):
      data = self.json('GET', route+('&' if '?' in route else '?')+f'per_page=100&page={page}')
      rows = data if key is None else data[key]
      result.extend(rows)
      if len(rows) < 100:
        return result
    raise ValueError('GitHub metadata pagination exceeded the operation bound')

  def source_json(self, source, path):
    data = self.json('GET', f'contents/{path}?ref={source}')
    if data['encoding'] != 'base64' or data['size'] > 1024**2:
      raise ValueError('native source profile is not bounded Git JSON')
    raw = base64.b64decode(data['content'], validate=False)
    if (len(raw) != data['size'] or hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest() != data['sha']):
      raise ValueError('native source profile differs from its Git blob')
    return decode(raw)

  def download(self, route, expected, destination=None):
    return transfer(['gh', 'api', '--hostname', 'github.com', f'repos/{REPO}/'+route,
        '-H', 'Accept: application/octet-stream'], expected, destination)

  def release(self, tag):
    value = self.json('GET', 'releases/tags/'+quote(tag, safe=''), missing=True)
    if value is not None:
      value['assets'] = self.collect(f'releases/{value["id"]}/assets')
    return value

  def pr_context(self, run, source, pr_number, job):
    trusted_run(run, source)
    if run['event'] != 'pull_request':
      raise ValueError('PR validation evidence cannot belong to another event type')
    if self.root is None:
      raise ValueError('PR validation evidence needs an owned publication directory')
    name = f'native-validation-context-{run["id"]}-{run["run_attempt"]}-{job}'
    artifacts = self.collect(f'actions/runs/{run["id"]}/artifacts', 'artifacts')
    selected = [row for row in artifacts if row['name'] == name and not row['expired']]
    if len(selected) != 1:
      raise ValueError(f'PR validation {name} requires one unexpired context artifact')
    selected = selected[0]
    pin = artifact_pin(selected, run, source, 1024**2)
    parent = self.root/'validation-contexts'
    parent.mkdir(exist_ok=True)
    if parent.is_symlink() or parent.resolve() != parent:
      raise ValueError('PR validation evidence directory is redirected')
    archive = parent/f'{run["id"]}-{run["run_attempt"]}-{job}.zip'
    if archive.is_symlink() or (archive.exists() and not archive.is_file()):
      raise ValueError('retained PR validation ZIP is redirected or not regular')
    if archive.exists():
      with archive.open('rb') as stream:
        if archive.stat().st_size != pin['bytes'] or hashlib.file_digest(stream, 'sha256').hexdigest() != pin['sha256']:
          raise ValueError('retained PR validation ZIP differs from its independent artifact pin')
    else:
      room(parent, pin['bytes'])
      self.download(f'actions/artifacts/{selected["id"]}/zip', pin, archive)
    bundle = Bundle(archive)
    try:
      if set(bundle.members) != {'validation-context.json'}:
        raise ValueError('PR validation artifact includes unexpected members')
      value = bundle.json('validation-context.json', 256*1024)
    finally:
      bundle.close()
    jobs = self.collect(f'actions/runs/{run["id"]}/jobs', 'jobs')
    actual = [row for row in jobs if row['name'] == PROOF_JOB_NAMES[job]
        and row['status'] == 'completed' and row['conclusion'] == 'success']
    if len(actual) != 1:
      raise ValueError('PR validation context needs one actual successful proof job')
    context = validation_context.verify(value, run['id'], run['run_attempt'], job,
        run['repository']['id'], source, pr_number, runner_job=actual[0])
    proof = dict(artifact_id=selected['id'], artifact_digest=selected['digest'],
        checkout_sha=value['checkout_sha'], checkout_tree=value['checkout_tree'],
        pr_number=context[0], base_sha=context[1])
    return {context}, proof


def trusted_run(run, source):
  if (run['head_sha'] != source or run['repository']['full_name'] != REPO
      or run['head_repository']['full_name'] != REPO):
    raise ValueError('native input provenance is not the requested same-repository source')


def artifact_pin(value, run, source, maximum):
  link = value['workflow_run']
  repository_id = run['repository']['id']
  if (value['expired'] or not positive(value['id']) or link['id'] != run['id'] or link['head_sha'] != source
      or link['repository_id'] != repository_id or link['head_repository_id'] != repository_id
      or not positive(value['size_in_bytes']) or value['size_in_bytes'] > maximum
      or not isinstance(value.get('digest'), str) or not value['digest'].startswith('sha256:')
      or not checksum(value['digest'][7:])):
    raise ValueError('native artifact identity/digest differs from the requested source')
  return dict(bytes=value['size_in_bytes'], sha256=value['digest'][7:])


def gate_job_allowed(filename, job):
  if filename == 'runtime-source-build.yml' and job.get('name') == FOUNDATION_ARM_JOB:
    # Only this native ARM job may cross the foundation's self-hosted boundary.
    # Labels/group/identity come from the Actions jobs API, not a claimed environment.
    return (runner_policy.job_is_allowed(job)
        and 'ubuntu-24.04-arm' in job['labels'] and 'self-hosted' not in job['labels']
        and type(job.get('runner_group_id')) is int and job['runner_group_id'] == 0
        and job.get('runner_group_name') == 'GitHub Actions'
        and type(job.get('runner_id')) is int and job['runner_id'] > 0
        and job['runner_name'] not in runner_policy.SELF_HOSTED_NAMES)
  return runner_policy.job_is_allowed(job, self_hosted_only=filename in SELF_HOSTED_GATES)


def gates(client, source, producer_id, pr_number):
  producer = client.json('GET', f'actions/runs/{producer_id}')
  trusted_run(producer, source)
  if (producer['path'] != PRODUCER or producer['event'] not in ('push', 'workflow_dispatch')
      or producer['status'] != 'completed' or producer['conclusion'] != 'success'):
    raise ValueError('native producer must have completed successfully for the exact source')
  runs = client.collect('actions/runs?head_sha='+source, 'workflow_runs')
  states, pr_contexts = [], []
  for filename, events, required in GATES:
    candidates = [r for r in runs if r['path'] == '.github/workflows/'+filename and r['event'] in events]
    is_pr = events == ('pull_request',)
    if is_pr:
      # Only the linked PR number is used here. Its current head/base fields
      # change after pushes and cannot prove the original workflow event.
      candidates = [r for r in candidates if any(isinstance(p, dict) and p.get('number') == pr_number
          for p in r.get('pull_requests') or [])]
    selected = max(candidates, key=lambda r: r['id']) if candidates else None
    contexts, proof = set(), None
    ready = False
    if selected is not None:
      trusted_run(selected, source)
      if selected['status'] == 'completed' and selected['conclusion'] == 'success':
        jobs = client.collect(f'actions/runs/{selected["id"]}/jobs', 'jobs')
        names = {j['name'] for j in jobs if j['status'] == 'completed' and j['conclusion'] == 'success'
            and gate_job_allowed(filename, j)}
        ready = required <= names and all(gate_job_allowed(filename, j)
            for j in jobs if j['conclusion'] == 'success')
        if ready and is_pr:
          contexts, proof = client.pr_context(selected, source, pr_number, PROOF_JOBS[filename])
    if is_pr:
      pr_contexts.append(contexts)
    states.append(dict(workflow=filename, events=events, run=None if selected is None else selected['id'],
        target_branch='dev' if is_pr else None,
        pull_requests=[dict(number=number, base_sha=base) for number, base in sorted(contexts)],
        validation_context=proof, ready=ready))
  # Individual green runs from different PRs or base commits do not prove one
  # coherent dev candidate. A missing PR context must also fail this intersection.
  if pr_contexts and not set.intersection(*pr_contexts):
    for state in states:
      if state['target_branch'] == 'dev':
        state['ready'] = False
  # The chosen producer itself must have run its real job, even if a newer
  # successful producer happens to exist for the same Git commit.
  jobs = client.collect(f'actions/runs/{producer_id}/jobs', 'jobs')
  if not any(j['name'] == 'Native compiler in pinned AGNOS ARM userspace'
      and runner_policy.job_is_allowed(j, self_hosted_only=True)
      and j['status'] == 'completed' and j['conclusion'] == 'success' for j in jobs):
    raise ValueError('selected native producer lacks its actual registered-runner job')
  return producer, states


def zip_directory_bound(path):
  size = path.stat().st_size
  with path.open('rb') as stream:
    stream.seek(max(0, size-65557))
    tail = stream.read()
    offset = tail.rfind(b'PK\x05\x06')
    if offset < 0 or len(tail)-offset < 22:
      raise ValueError('native artifact lacks its ZIP end record')
    record = struct.unpack_from('<4s4H2LH', tail, offset)
    if record[1] or record[2] or offset+22+record[-1] != len(tail):
      raise ValueError('split native artifact ZIP is unsupported')
    entries, length, start = record[4], record[5], record[6]
    if 65535 in (record[3], entries) or 0xffffffff in (length, start):
      locator = offset-20
      if locator < 0 or tail[locator:locator+4] != b'PK\x06\x07':
        raise ValueError('native ZIP64 locator missing')
      position = struct.unpack_from('<Q', tail, locator+8)[0]
      if position > size-56:
        raise ValueError('native ZIP64 position escaped the artifact')
      stream.seek(position)
      record64 = stream.read(56)
      if record64[:4] != b'PK\x06\x06' or struct.unpack_from('<II', record64, 16) != (0, 0):
        raise ValueError('invalid native ZIP64 end record')
      entries, length, start = struct.unpack_from('<QQQ', record64, 32)
    if entries > 100000 or length > 16*1024**2 or start+length > size:
      raise ValueError('native artifact central directory exceeds the bounded reader')


class Bundle:
  def __init__(self, path):
    zip_directory_bound(path)
    self.zip = zipfile.ZipFile(path)
    self.members = {}
    for item in self.zip.infolist():
      name = PurePosixPath(item.filename)
      if (name.is_absolute() or '..' in name.parts or str(name) != item.filename.rstrip('/')
          or item.filename in self.members):
        raise ValueError('unsafe or duplicate native artifact path')
      self.members[item.filename] = item

  def close(self):
    self.zip.close()

  def member(self, name):
    item = self.members[name]
    kind = stat.S_IFMT(item.external_attr >> 16)
    if (item.is_dir() or kind not in (0, stat.S_IFREG) or item.flag_bits & 1
        or item.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)):
      raise ValueError('native publication member is not a regular file')
    return item

  def json(self, name, maximum=1024**2):
    item = self.member(name)
    if item.file_size > maximum:
      raise ValueError('native publication JSON exceeds its role bound')
    with self.zip.open(item) as stream:
      return decode(stream.read(maximum+1))


def validate_bundle(bundle, source, run, index_pin, profiles, sdk_plan, source_tree):
  manifest = bundle.json(PREFIX+'publication-candidate.json')
  index_name = PREFIX+'native-bootstrap-inputs.candidate.json'
  index = bundle.json(index_name)
  with bundle.zip.open(bundle.member(index_name)) as stream:
    if hashlib.file_digest(stream, 'sha256').hexdigest() != index_pin:
      raise ValueError('native index differs from its independently selected pin')
  if (type(manifest['schema']) is not int or manifest['schema'] != 1 or manifest['workflow_source_commit'] != source
      or not positive(manifest['source_run_id']) or not positive(manifest['source_run_attempt'])
      or manifest['source_run_id'] != run['id'] or manifest['source_run_attempt'] != run['run_attempt']
      or manifest['source_job'] != 'compiler' or manifest['index_sha256'] != index_pin
      or manifest['published_release'] is not False or manifest['committed_input_index'] is not False
      or manifest['physical_device_execution'] is not False or manifest['source_index_resolver_cases_passed'] != CASES):
    raise ValueError('native publication provenance or completed cases differ')
  if (set(index) != {'schema', 'seed', 'system_sdk', 'providers'} or type(index['schema']) is not int
      or index['schema'] != 1 or set(index['providers']) != set(ROLES)):
    raise ValueError('native publication index is incomplete')
  summary = bundle.json(PREFIX+'summary.json', 4*1024**2)
  if (summary['source_commit'] != source or summary['production_source_index_resolver_executed'] is not True
      or summary['provider_roles'] != 15 or summary['runtime_roles'] != 18 or summary['compiler_input_roles'] != 9
      or summary['published_release'] is not False or summary['input_index_committed'] is not False
      or summary['input_index_candidate_sha256'] != index_pin
      or set(summary['input_index_cases']) != set(CASES)
      or any(row['index_sha256'] != index_pin for row in summary['input_index_cases'].values())):
    raise ValueError('native resolver proof is incomplete')
  for case in ['initial', 'reused', 'docs', *CASES]:
    sample = summary['metrics'][case]
    if sample['state'] != 'success' or sample['memory']['kernel_peak_bytes'] <= 0:
      raise ValueError('native preparation lacks successful real-controller evidence')
  environment = bundle.json('native-agnos-environment-599.json', 16*1024**2)
  generation = environment['native_complete_generation']
  if (environment['source_commit'] != source or generation['source']['commit'] != source
      or generation['binaries'] != 99 or generation['compiler_groups'] != 13
      or generation['source_built_sealer_and_abi_audit_passed'] is not True
      or generation['source_generation_verified'] is not True or generation['runtime_activated'] is not False
      or generation['runtime_started'] is not False or generation['physical_device_execution'] is not False):
    raise ValueError('native publication lacks the full sealed-source engineering result')
  files, job_root = {}, None
  for item in manifest['files']:
    if set(item) != {'url', 'sha256', 'bytes', 'path'}:
      raise ValueError('unexpected native publication file fields')
    pin = {k: item[k] for k in ('url', 'sha256', 'bytes')}
    asset(pin)
    path = PurePosixPath(item['path'])
    if (not path.is_absolute() or '..' in path.parts or str(path) != item['path']
        or len(path.parts) < 4 or path.parts[1] != 'tmp'
        or not re.fullmatch(r'openpilot-ci\.[A-Za-z0-9]+', path.parts[2])):
      raise ValueError('native publication path escaped the job-owned artifact')
    root = path.parts[:3]
    if job_root is not None and root != job_root:
      raise ValueError('native publication files belong to different jobs')
    job_root = root
    member = '/'.join(path.parts[3:])
    if item['url'] in files or bundle.member(member).file_size != pin['bytes']:
      raise ValueError('duplicate or size-mismatched native publication member')
    files[item['url']] = dict(**pin, member=member)
  expected = {}
  def require(pin, maximum=2*1024**3):
    asset(pin, maximum)
    if pin['url'] in expected or pin['url'] not in files:
      raise ValueError('missing or duplicate required native publication asset')
    row = files[pin['url']]
    if any(row[k] != pin[k] for k in ('sha256', 'bytes')):
      raise ValueError('native publication differs from its role descriptor')
    expected[pin['url']] = row
    return row['member']
  seed = bundle.json(require(index['seed'], 1024**2))
  if (type(seed['schema']) is not int or seed['schema'] != 1
      or seed['role'] != 'openpilot-runtime-inputs' or seed['source'] != dict(commit=source, tree=source_tree)
      or seed['target'] != 'aarch64-unknown-linux-gnu' or seed['agnos'] != '19.8-carrot-bt1'
      or seed['protocols'] != dict(current_tools=1, request_owner=2, generation_boot=1)
      or any(type(v) is not int for v in seed['protocols'].values())
      or set(seed['assets']) != {'helper', 'notices'}
      or index['seed']['url'] != BASE+'native-bootstrap-'+source+'/manifest.json'):
    raise ValueError('native seed source or generation-boot protocol differs')
  for role, filename in [('helper', 'openpilot-runtime-inputs-aarch64'), ('notices', 'THIRD_PARTY_NOTICES.native-bootstrap.txt')]:
    if seed['assets'][role]['url'] != BASE+'native-bootstrap-'+source+'/'+filename:
      raise ValueError('native seed asset URL escaped its source release')
  require(seed['assets']['helper'], 64*1024**2)
  require(seed['assets']['notices'], 16*1024**2)
  sdk = bundle.json(require(index['system_sdk'], 16*1024**2), 16*1024**2)
  if (sdk['kind'] != 'agnos-system-sdk' or sdk['plan'] != sdk_plan
      or index['system_sdk']['url'] != BASE+'native-system-sdk-'+index['system_sdk']['sha256']+'/system-sdk.json'):
    raise ValueError('native SDK descriptor differs from the tested source')
  for role in ROLES:
    descriptor = bundle.json(require(index['providers'][role], 1024**2))
    if (descriptor['profile'] != profiles[role]['profile']
        or descriptor['provider_source_commit'] != profiles[role]['source_commit']
        or index['providers'][role]['url'] != descriptor['asset']['url'].rsplit('/', 1)[0]+'/provider.json'):
      raise ValueError('native provider descriptor differs from the source profile')
    require(descriptor['asset'])
  if set(files) != set(expected) or len(expected) != 34 or sum(r['bytes'] for r in expected.values()) > 4*1024**3:
    raise ValueError('native publication includes foreign files or exceeds this operation envelope')
  return index, list(expected.values())


def stage(bundle, files, root):
  room(root, sum(r['bytes'] for r in files))
  result = []
  for row in files:
    tag, name = asset({k: row[k] for k in ('url', 'sha256', 'bytes')})
    parent = root/tag
    parent.mkdir(exist_ok=True)
    if parent.is_symlink() or parent.resolve() != parent:
      raise ValueError('native publication staging directory is redirected')
    destination = parent/name
    digest, count = hashlib.sha256(), 0
    with bundle.zip.open(bundle.member(row['member'])) as source, destination.open('xb') as output:
      while chunk := source.read(65536):
        count += len(chunk)
        if count > row['bytes']:
          raise ValueError('native publication member exceeded its declared size')
        digest.update(chunk)
        output.write(chunk)
      output.flush()
      os.fsync(output.fileno())
    if count != row['bytes'] or digest.hexdigest() != row['sha256']:
      raise ValueError('native publication member failed SHA/size verification')
    destination.chmod(0o444)
    result.append(dict(**row, tag=tag, name=name, path=str(destination)))
  return result


def remote_asset(client, release, row, allow_starter=False):
  matches = [a for a in release['assets'] if a['name'] == row['name']]
  if len(matches) > 1:
    raise ValueError('duplicate remote native asset name')
  if not matches:
    return None
  value = matches[0]
  if allow_starter and value['state'] == 'starter' and value['size'] == 0 and not value.get('digest'):
    return None
  if value['state'] != 'uploaded' or value['size'] != row['bytes']:
    raise ValueError('existing native release asset has different size or incomplete state; preserved')
  if value.get('digest'):
    if value['digest'] != 'sha256:'+row['sha256']:
      raise ValueError('existing native release asset has a different digest; preserved')
  else:
    client.download(f'releases/assets/{value["id"]}', row)
  return value


def owner_body(source, index_pin):
  return f'{MARKER}\nsource_commit={source}\nindex_sha256={index_pin}\n'


def check_new_tag(client, tag, source):
  value = client.json('GET', 'git/ref/tags/'+quote(tag, safe=''), missing=True)
  if value is None:
    return
  target = value['object']
  for _ in range(4):
    if target['type'] != 'tag':
      break
    target = client.json('GET', 'git/tags/'+target['sha'])['object']
  if target['type'] != 'commit' or target['sha'] != source:
    raise ValueError('existing native tag points to a different source; preserved')


def inspect_releases(client, rows, source, index_pin):
  groups = {}
  for row in rows:
    groups.setdefault(row['tag'], []).append(row)
  result = []
  for tag, files in groups.items():
    release = client.release(tag)
    if release is None:
      check_new_tag(client, tag, source)
    if release is not None and release['draft']:
      if (release['body'] != owner_body(source, index_pin)
          or not {a['name'] for a in release['assets']} <= {r['name'] for r in files}):
        raise ValueError('existing draft belongs to a different publication; preserved')
    missing = [row['name'] for row in files if release is None
        or remote_asset(client, release, row, allow_starter=release['draft']) is None]
    result.append(dict(tag=tag, release_id=None if release is None else release['id'],
        draft=True if release is None else release['draft'], missing=missing))
  return result


def upload(client, tag, row, source, index_pin):
  # No --clobber: a published or staging asset is never silently replaced.
  for attempt in range(3):
    release = client.release(tag)
    if release['draft'] and release['body'] != owner_body(source, index_pin):
      raise ValueError('native draft ownership changed before upload')
    # GitHub can leave a zero-byte starter after a lost upload. Only our own
    # unpublished staging slot can be removed; complete/public assets survive.
    starters = [a for a in release['assets'] if a['name'] == row['name'] and a['state'] == 'starter'
        and a['size'] == 0 and not a.get('digest')]
    if release['draft'] and starters:
      if len(starters) != 1:
        raise ValueError('duplicate native upload staging slot')
      client.json('DELETE', f'releases/assets/{starters[0]["id"]}')
      release = client.release(tag)
    if remote_asset(client, release, row) is not None:
      return
    try:
      result = subprocess.run(['gh', 'release', 'upload', tag, row['path'], '--repo', REPO],
          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=1200, check=False)
    except (OSError, subprocess.SubprocessError):
      result = None
    # A timeout/failed response can follow a successful server-side upload.
    # Re-read its immutable identity before deciding whether to send any bytes.
    release = client.release(tag)
    if remote_asset(client, release, row, allow_starter=release['draft']) is not None:
      return
    if result is not None and result.returncode == 0:
      raise ValueError('native upload returned success without a verified release asset')
    if attempt < 2:
      time.sleep(2**attempt)
  raise ValueError('native upload failed; verified remote assets retained for resume')


def publish(client, rows, source, index_pin, states, report, persist):
  if not all(s['ready'] for s in states):
    raise ValueError('native input publication requires every exact-source gate')
  inspect_releases(client, rows, source, index_pin)  # Reject every conflict before any mutation.
  groups = {}
  for row in rows:
    groups.setdefault(row['tag'], []).append(row)
  for tag, files in groups.items():
    release = client.release(tag)
    if release is None:
      try:
        client.json('POST', 'releases', dict(tag_name=tag, target_commitish=source,
            name='Native input '+tag, body=owner_body(source, index_pin), draft=True, prerelease=True))
      except (ValueError, OSError, subprocess.SubprocessError):
        # Creation may have succeeded before the response was lost. Ownership
        # and every existing asset are checked again before any resume.
        if client.release(tag) is None:
          raise
    inspect_releases(client, files, source, index_pin)
    for row in files:
      upload(client, tag, row, source, index_pin)
    report['remote_releases'] = inspect_releases(client, rows, source, index_pin)
    persist()
  # All groups are staged and verified before making any newly created release
  # public. Partial publication preserves the same bytes and can be resumed.
  for state in inspect_releases(client, rows, source, index_pin):
    if state['missing']:
      raise ValueError('native publication still lacks a required immutable asset')
    if state['draft']:
      client.json('PATCH', f'releases/{state["release_id"]}', dict(draft=False, make_latest='false'))
    report['remote_releases'] = inspect_releases(client, rows, source, index_pin)
    persist()
  report['published_release'] = True
  report['public_downloads_verified'] = False
  persist()
  total = 0
  for row in rows:
    total += transfer(['curl', '--proto', '=https', '--proto-redir', '=https', '--location', '--fail',
        '--silent', '--show-error', '--connect-timeout', '20', '--max-time', '600', row['url']], row)
  report['public_transfer_bytes'] = total
  report['public_downloads_verified'] = True
  persist()


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source-run-id', required=True, type=int)
  parser.add_argument('--source-pr-number', required=True, type=int)
  parser.add_argument('--source-commit', required=True)
  parser.add_argument('--index-sha256', required=True)
  parser.add_argument('--publish', action='store_true')
  args = parser.parse_args()
  if (not runner_policy.environment_allowed(os.environ, self_hosted_only=True)
      or os.environ.get('GITHUB_REPOSITORY') != REPO
      or os.environ.get('GITHUB_SERVER_URL') != 'https://github.com'
      or os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch'
      or not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode)):
    raise ValueError('native input publication requires the registered manual Actions context and isolated Python')
  if (not positive(args.source_run_id) or not positive(args.source_pr_number)
      or not checksum(args.source_commit, 40) or not checksum(args.index_sha256)):
    raise ValueError('native publication needs an exact source run/commit/index pin')
  job = Path(os.environ['CI_JOB_TEMP'])
  identity = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.') or job.is_symlink()
      or job.resolve() != job or job.stat().st_uid != os.geteuid() or (job/'owner').read_text() != identity):
    raise ValueError('native publication work is not owned by this job')
  root = job/'native-input-publication'
  root.mkdir()
  started, free = time.monotonic(), shutil.disk_usage(root).free
  report = dict(schema=1, publisher_source=os.environ['GITHUB_SHA'], source_commit=args.source_commit,
      source_run_id=args.source_run_id, source_pr_number=args.source_pr_number,
      index_sha256=args.index_sha256, mode='publish' if args.publish else 'verify',
      published_release=False, committed_input_index=False, runtime_activated=False, device_contacted=False)
  def persist():
    report.update(elapsed_seconds=time.monotonic()-started, own_kernel_peak_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*1024,
        filesystem_growth_bytes=free-shutil.disk_usage(root).free)
    pending = root/'report.pending.json'
    pending.write_text(json.dumps(report, indent=2)+'\n')
    os.replace(pending, root/'report.json')
  try:
    client = GitHub(root)
    producer, states = gates(client, args.source_commit, args.source_run_id, args.source_pr_number)
    report.update(source_run_attempt=producer['run_attempt'], gates=states,
        gates_ready=all(s['ready'] for s in states), ready_to_publish=False)
    persist()
    if args.publish and not report['gates_ready']:
      raise ValueError('native input publication requires every exact-source gate')
    artifacts = client.collect(f'actions/runs/{args.source_run_id}/artifacts', 'artifacts')
    name = 'native-input-payload-'+args.source_commit+'-'+str(producer['run_attempt'])
    candidates = [a for a in artifacts if a['name'] == name and not a['expired']]
    if len(candidates) != 1:
      raise ValueError('native producer requires one unexpired input-only payload artifact')
    selected = candidates[0]
    pin = artifact_pin(selected, producer, args.source_commit, 8*1024**3)
    room(root, selected['size_in_bytes'])
    report.update(artifact_id=selected['id'], artifact_digest=selected['digest'])
    persist()
    archive = root/'producer.zip'
    client.download(f'actions/artifacts/{selected["id"]}/zip', pin, archive)
    profiles = {role: client.source_json(args.source_commit, f'rust/native-{role}-provider.json') for role in ROLES}
    sdk_plan = client.source_json(args.source_commit, 'rust/native-system-sdk.json')
    commit = client.json('GET', 'git/commits/'+args.source_commit)
    if commit['sha'] != args.source_commit or not checksum(commit['tree']['sha'], 40):
      raise ValueError('native source commit/tree identity differs')
    bundle = Bundle(archive)
    try:
      _, files = validate_bundle(bundle, args.source_commit, producer, args.index_sha256, profiles, sdk_plan, commit['tree']['sha'])
      staged = root/'verified-assets'
      staged.mkdir()
      rows = stage(bundle, files, staged)
      raw_index = bundle.zip.read(PREFIX+'native-bootstrap-inputs.candidate.json')
    finally:
      bundle.close()
    report.update(artifact_id=selected['id'], artifact_digest=selected['digest'], assets_verified=True,
        source_tree=commit['tree']['sha'], staged_bytes=sum(r['bytes'] for r in rows),
        ready_to_publish=report['gates_ready'],
        remote_releases=inspect_releases(client, rows, args.source_commit, args.index_sha256))
    (root/'native-bootstrap-inputs.candidate.json').write_bytes(raw_index)
    persist()
    if args.publish:
      current_producer, current_states = gates(client, args.source_commit, args.source_run_id, args.source_pr_number)
      report.update(gates=current_states, gates_ready=all(s['ready'] for s in current_states),
          ready_to_publish=False)
      persist()
      if current_producer['run_attempt'] != producer['run_attempt']:
        raise ValueError('native producer attempt changed during input verification; assets retained')
      report['ready_to_publish'] = report['gates_ready']
      persist()
      publish(client, rows, args.source_commit, args.index_sha256, current_states, report, persist)
      (root/'native-bootstrap-inputs.json').write_bytes(raw_index)
    report['state'] = 'success'
    persist()
    print(json.dumps(report, indent=2))
  except BaseException:
    report['state'] = 'failure'
    persist()
    raise


if __name__ == '__main__':
  try:
    main()
  except (ValueError, TypeError, KeyError, OSError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
    print('native input publication: '+str(error), file=sys.stderr)
    sys.exit(1)

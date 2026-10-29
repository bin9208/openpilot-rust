"""Owned ZIP/API fixtures are never executed or sent to GitHub releases."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

_SPEC = importlib.util.spec_from_file_location('owned_native_publication', Path(__file__).resolve().parents[2]/'native/publish_inputs.py')
publication = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(publication)
_PAYLOAD = importlib.util.spec_from_file_location('owned_input_payload', Path(__file__).resolve().parents[1]/'stage_native_input_payload.py')
payload = importlib.util.module_from_spec(_PAYLOAD)
_PAYLOAD.loader.exec_module(payload)

SOURCE, TREE = 'a'*40, 'b'*40


def context_fixture(run_id=42, job='workspace', base=TREE, base_ref='dev', *,
                    runner_name='bin-actions', runner_environment='self-hosted'):
  raw = (f'tree {"c"*40}\nparent {base}\nparent {SOURCE}\n'
      'author Owned <ci@example.invalid> 1 +0000\ncommitter Owned <ci@example.invalid> 1 +0000\n\nOwned merge proof\n').encode()
  merge = hashlib.sha1(b'commit '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
  repository = dict(id=7, full_name=publication.REPO)
  event = dict(number=611, repository=repository.copy(), pull_request=dict(
      body='Owned PR body must not be retained',
      base=dict(ref=base_ref, sha=base, repo=repository.copy()), head=dict(sha=SOURCE, repo=repository.copy())))
  env = dict(GITHUB_REPOSITORY=publication.REPO, GITHUB_REPOSITORY_ID='7', GITHUB_RUN_ID=str(run_id),
      GITHUB_RUN_ATTEMPT='1', GITHUB_JOB=job, GITHUB_EVENT_NAME='pull_request', RUNNER_NAME=runner_name,
      RUNNER_ENVIRONMENT=runner_environment, GITHUB_ACTIONS='true', RUNNER_OS='Linux', RUNNER_ARCH='X64',
      GITHUB_REF='refs/pull/611/merge', GITHUB_SHA=merge)
  return publication.validation_context.receipt(event, env, raw)


def job_fixture(name, runner_name='bin-actions', labels=None):
  return dict(name=name, status='completed', conclusion='success', runner_name=runner_name,
      labels=['self-hosted', 'Linux', 'X64'] if labels is None else labels)


def encode(value):
  return (json.dumps(value, sort_keys=True, indent=2)+'\n').encode()


class Fixture:
  def __init__(self, root):
    self.root, self.entries, self.files = root, {}, {}
    self.run = dict(id=42, run_attempt=1, head_sha=SOURCE, path=publication.PRODUCER, event='push',
        repository=dict(full_name=publication.REPO, id=7), head_repository=dict(full_name=publication.REPO),
        status='completed', conclusion='success')
    seed_tag = 'native-bootstrap-'+SOURCE
    helper = self.put(seed_tag, 'openpilot-runtime-inputs-aarch64', b'Owned non-executed helper fixture')
    notice = self.put(seed_tag, 'THIRD_PARTY_NOTICES.native-bootstrap.txt', b'Owned license fixture')
    seed = dict(schema=1, role='openpilot-runtime-inputs', source=dict(commit=SOURCE, tree=TREE),
        target='aarch64-unknown-linux-gnu', agnos='19.8-carrot-bt1',
        protocols=dict(current_tools=1, request_owner=2, generation_boot=1), assets=dict(helper=helper, notices=notice))
    self.sdk_plan = dict(schema=1, factory_image_sha256='c'*64)
    sdk = encode(dict(kind='agnos-system-sdk', plan=self.sdk_plan))
    sdk_tag = 'native-system-sdk-'+hashlib.sha256(sdk).hexdigest()
    self.index = dict(schema=1, seed=self.put(seed_tag, 'manifest.json', encode(seed)),
        system_sdk=self.put(sdk_tag, 'system-sdk.json', sdk), providers={})
    self.profiles = {}
    for role in publication.ROLES:
      tag = 'native-owned-'+role
      archive = self.put(tag, 'component.tar.zst', ('Owned component '+role).encode())
      self.profiles[role] = dict(profile=role+'-owned', source_commit='d'*40)
      self.index['providers'][role] = self.put(tag, 'provider.json', encode(dict(
          profile=self.profiles[role]['profile'], provider_source_commit='d'*40, asset=archive)))
    self.index_bytes = encode(self.index)
    self.pin = hashlib.sha256(self.index_bytes).hexdigest()
    self.entries[publication.PREFIX+'native-bootstrap-inputs.candidate.json'] = self.index_bytes
    self.manifest = dict(schema=1, workflow_source_commit=SOURCE, source_run_id=42, source_run_attempt=1,
        source_job='compiler', index_sha256=self.pin, files=list(self.files.values()), published_release=False,
        committed_input_index=False, physical_device_execution=False, source_index_resolver_cases_passed=publication.CASES)
    self.summary = dict(source_commit=SOURCE, production_source_index_resolver_executed=True,
        provider_roles=15, runtime_roles=18, compiler_input_roles=9, published_release=False,
        input_index_committed=False, input_index_candidate_sha256=self.pin,
        input_index_cases={case:dict(index_sha256=self.pin) for case in publication.CASES},
        metrics={case:dict(state='success', memory=dict(kernel_peak_bytes=1))
          for case in ['initial', 'reused', 'docs', *publication.CASES]})
    self.environment = dict(source_commit=SOURCE, native_complete_generation=dict(source=dict(commit=SOURCE),
        binaries=99, compiler_groups=13, source_built_sealer_and_abi_audit_passed=True,
        source_generation_verified=True, runtime_activated=False, runtime_started=False, physical_device_execution=False))

  def put(self, tag, name, data):
    url = publication.BASE+tag+'/'+name
    member = 'native-agnos/owned-'+tag+'/'+name
    pin = dict(url=url, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    self.entries[member] = data
    self.files[url] = dict(**pin, path='/tmp/openpilot-ci.Owned/'+member)
    return pin

  def write(self):
    self.entries[publication.PREFIX+'publication-candidate.json'] = encode(self.manifest)
    self.entries[publication.PREFIX+'summary.json'] = encode(self.summary)
    self.entries['native-agnos-environment-599.json'] = encode(self.environment)
    path = self.root/'artifact.zip'
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
      for name, data in self.entries.items():
        archive.writestr(name, data)
    return path

  def validated(self):
    bundle = publication.Bundle(self.write())
    try:
      return publication.validate_bundle(bundle, SOURCE, self.run, self.pin, self.profiles, self.sdk_plan, TREE)
    finally:
      bundle.close()

  def staged(self):
    bundle = publication.Bundle(self.write())
    try:
      _, files = publication.validate_bundle(bundle, SOURCE, self.run, self.pin, self.profiles, self.sdk_plan, TREE)
      root = self.root/'staged'
      root.mkdir()
      return publication.stage(bundle, files, root)
    finally:
      bundle.close()


class Releases:
  def __init__(self):
    self.values, self.mutations, self.fail_patch = {}, [], None

  def release(self, tag):
    return copy.deepcopy(self.values.get(tag))

  def json(self, method, route, value=None, **kwargs):
    if method == 'GET':
      return None
    self.mutations.append((method, route))
    if method == 'POST':
      self.values[value['tag_name']] = dict(id=len(self.values)+1, draft=True, body=value['body'], assets=[])
    elif method == 'PATCH':
      number = int(route.split('/')[-1])
      if self.fail_patch == number:
        raise ValueError('Owned interrupted publication')
      next(r for r in self.values.values() if r['id'] == number)['draft'] = False
    elif method == 'DELETE':
      number = int(route.split('/')[-1])
      for release in self.values.values():
        release['assets'] = [a for a in release['assets'] if a['id'] != number]
    return {}

  def add(self, tag, row):
    release = self.values[tag]
    if not any(a['name'] == row['name'] for a in release['assets']):
      release['assets'].append(dict(id=1000+len(release['assets']), name=row['name'], state='uploaded',
          size=row['bytes'], digest='sha256:'+row['sha256']))


class PublicationTests(unittest.TestCase):
  def test_input_only_payload_links_exact_files_without_copying_or_following_a_redirect(self):
    for redirected in (False, True):
      with self.subTest(redirected=redirected), tempfile.TemporaryDirectory() as temp:
        base = Path(temp)
        root = base/'job'
        root.mkdir()
        f = Fixture(root)
        for row in f.manifest['files']:
          relative = Path(row['path']).parts[3:]
          row['path'] = str(root.joinpath(*relative))
        f.write()
        for name, data in f.entries.items():
          path = root/name
          path.parent.mkdir(parents=True, exist_ok=True)
          path.write_bytes(data)
        first = Path(f.manifest['files'][0]['path'])
        if redirected:
          outside = base/'outside'
          outside.write_bytes(first.read_bytes())
          first.unlink()
          first.symlink_to(outside)
          with self.assertRaisesRegex(ValueError, 'outside'):
            payload.stage(root, SOURCE, 42, 1)
          self.assertFalse((root/'native-input-payload').exists())
          self.assertEqual(outside.read_bytes(), f.entries[str(first.relative_to(root))])
        else:
          result = payload.stage(root, SOURCE, 42, 1)
          self.assertEqual(result['files'], 38)
          self.assertEqual(result['copied_bytes'], 0)
          target = root/'native-input-payload'/first.relative_to(root)
          self.assertEqual(first.stat().st_ino, target.stat().st_ino)
          self.assertEqual(first.read_bytes(), target.read_bytes())

  def test_entry_refuses_non_actions_context_before_any_api_or_file_preparation(self):
    with mock.patch.dict(os.environ, {}, clear=True), \
         mock.patch('sys.argv', ['publish_inputs.py', '--source-run-id', '42', '--source-pr-number', '611', '--source-commit', SOURCE, '--index-sha256', 'b'*64]), \
         mock.patch.object(publication, 'GitHub') as client:
      with self.assertRaisesRegex(ValueError, 'registered manual Actions context'):
        publication.main()
    client.assert_not_called()

  def test_stream_is_bounded_and_cancels_only_its_new_process_group(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      payload = b'Owned transfer bytes'
      output = root/'artifact'
      publication.transfer([sys.executable, '-I', '-B', '-S', '-c',
          'import sys;sys.stdout.buffer.write(bytes.fromhex(sys.argv[1]))', payload.hex()],
          dict(bytes=len(payload), sha256=hashlib.sha256(payload).hexdigest()), output)
      self.assertEqual(output.read_bytes(), payload)
      innocent = subprocess.Popen([sys.executable, '-I', '-B', '-S', '-c', 'import time;time.sleep(60)'])
      try:
        pid = root/'child-pid'
        script = 'import os,sys,time;from pathlib import Path;Path(sys.argv[1]).write_text(str(os.getpid()));sys.stdout.buffer.write(b"X"*5000);sys.stdout.flush();time.sleep(60)'
        with self.assertRaisesRegex(ValueError, 'exceeded'):
          publication.transfer([sys.executable, '-I', '-B', '-S', '-c', script, str(pid)],
              dict(bytes=1, sha256='a'*64))
        self.assertIsNone(innocent.poll())
        self.assertFalse(Path('/proc/'+pid.read_text()).exists())
      finally:
        innocent.terminate()
        innocent.wait(timeout=5)
      with mock.patch.object(publication.subprocess, 'Popen') as child:
        with self.assertRaises(FileExistsError):
          publication.transfer(['/owned/not-executed'], dict(bytes=1, sha256='a'*64), output)
      child.assert_not_called()

  def test_actual_role_selection_stages_only_34_hash_verified_nonexecuted_files(self):
    with tempfile.TemporaryDirectory() as temp:
      fixture = Fixture(Path(temp))
      rows = fixture.staged()
      self.assertEqual(len(rows), 34)
      self.assertEqual({row['url'] for row in rows}, set(fixture.files))
      for row in rows:
        path = Path(row['path'])
        self.assertEqual(path.stat().st_mode & 0o777, 0o444)
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), row['sha256'])

  def test_wrong_source_attempt_index_case_or_generation_never_reaches_publication(self):
    for fault in ('source', 'attempt', 'index', 'cases', 'generation', 'extra-file', 'foreign-url', 'path'):
      with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temp:
        f = Fixture(Path(temp))
        if fault == 'source': f.manifest['workflow_source_commit'] = 'e'*40
        if fault == 'attempt': f.manifest['source_run_attempt'] = True
        if fault == 'index': f.pin = 'e'*64
        if fault == 'cases': f.summary['metrics']['git-index-docs']['state'] = 'failure'
        if fault == 'generation': f.environment['native_complete_generation']['binaries'] = 1
        if fault == 'extra-file':
          extra = f.put('native-foreign', 'capture.bin', b'Owned forbidden extra')
          f.manifest['files'].append(f.files[extra['url']])
        if fault == 'foreign-url': f.manifest['files'][0]['url'] = 'https://elsewhere.invalid/payload'
        if fault == 'path': f.manifest['files'][0]['path'] = '/tmp/openpilot-ci.Owned/../other/data'
        with self.assertRaises((ValueError, KeyError)):
          f.validated()
        self.assertFalse((f.root/'staged').exists())

  def test_member_corruption_and_zip_redirect_do_not_certify_themselves(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      name = next(iter(f.entries))
      body = f.entries[name]
      f.entries[name] = b'X'+body[1:]
      with self.assertRaisesRegex(ValueError, 'SHA/size'):
        f.staged()
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      f.entries['../other-session/capture'] = b'Owned path escape'
      with self.assertRaisesRegex(ValueError, 'unsafe'):
        f.validated()

  def test_pending_gate_and_remote_conflicts_preserve_every_existing_asset(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      rows, client = f.staged(), Releases()
      with self.assertRaisesRegex(ValueError, 'every exact-source gate'):
        publication.publish(client, rows, SOURCE, f.pin, [dict(ready=False)], {}, lambda:None)
      self.assertEqual(client.mutations, [])

      row = rows[-1]
      client.values[row['tag']] = dict(id=1, draft=False, body='', assets=[dict(id=1, name=row['name'],
          size=row['bytes'], state='uploaded', digest='sha256:'+'f'*64)])
      original = copy.deepcopy(client.values)
      with self.assertRaisesRegex(ValueError, 'different digest'):
        publication.publish(client, rows, SOURCE, f.pin, [dict(ready=True)], {}, lambda:None)
      self.assertEqual(client.values, original)
      self.assertEqual(client.mutations, [])

  def test_an_existing_foreign_git_tag_cannot_be_retargeted_by_release_creation(self):
    client = mock.Mock()
    client.json.return_value = dict(object=dict(type='commit', sha='f'*40))
    with self.assertRaisesRegex(ValueError, 'different source'):
      publication.check_new_tag(client, 'native-owned', SOURCE)
    self.assertEqual(client.json.call_args.args[0], 'GET')

  def test_interrupted_publication_resumes_verified_bytes_without_replacing_public_assets(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      rows, client = f.staged(), Releases()
      report = dict(published_release=False)
      def upload(api, tag, row, *_): api.add(tag, row)
      with mock.patch.object(publication, 'upload', side_effect=upload), \
           mock.patch.object(publication, 'transfer', side_effect=lambda command, row:row['bytes']) as read:
        client.fail_patch = 2
        with self.assertRaisesRegex(ValueError, 'interrupted'):
          publication.publish(client, rows, SOURCE, f.pin, [dict(ready=True)], report, lambda:None)
        read.assert_not_called()
        self.assertFalse(client.values[rows[0]['tag']]['draft'])
        self.assertFalse(report['published_release'])
        published_assets = copy.deepcopy(client.values[rows[0]['tag']]['assets'])
        client.fail_patch = None
        publication.publish(client, rows, SOURCE, f.pin, [dict(ready=True)], report, lambda:None)
      self.assertTrue(report['published_release'])
      self.assertEqual(report['public_transfer_bytes'], sum(r['bytes'] for r in rows))
      self.assertEqual(client.values[rows[0]['tag']]['assets'], published_assets)
      self.assertFalse(any(r['draft'] for r in client.values.values()))
      self.assertFalse(any(method == 'DELETE' for method, _ in client.mutations))

  def test_lost_upload_response_and_only_owned_empty_draft_slot_are_recoverable(self):
    row = dict(tag='native-owned', name='owned.bin', path='/owned/not-executed', bytes=1, sha256='a'*64)
    client = Releases()
    client.values[row['tag']] = dict(id=1, draft=True, body=publication.owner_body(SOURCE, 'b'*64),
        assets=[dict(id=99, name=row['name'], state='starter', size=0, digest=None)])
    def interrupted(*args, **kwargs):
      client.add(row['tag'], row)
      raise subprocess.TimeoutExpired('owned mocked upload', 1)
    with mock.patch.object(publication.subprocess, 'run', side_effect=interrupted) as command:
      publication.upload(client, row['tag'], row, SOURCE, 'b'*64)
    self.assertEqual(command.call_count, 1)
    self.assertEqual(client.mutations, [('DELETE', 'releases/assets/99')])
    client.values[row['tag']]['body'] = 'Someone else owns this draft'
    with mock.patch.object(publication.subprocess, 'run') as command:
      with self.assertRaisesRegex(ValueError, 'ownership'):
        publication.upload(client, row['tag'], row, SOURCE, 'b'*64)
    command.assert_not_called()


class ContextEvidenceTests(unittest.TestCase):
  def test_retained_runner_identity_is_bound_to_actual_api_name_and_labels(self):
    for name, environment, labels in [
        ('bin-actions', 'self-hosted', ['self-hosted', 'Linux', 'X64']),
        ('bin-actions2', 'self-hosted', ['self-hosted', 'Linux', 'X64']),
        ('bin-actions3', 'self-hosted', ['self-hosted', 'Linux', 'X64']),
        ('GitHub Actions owned X64', 'github-hosted', ['ubuntu-24.04']),
        ('GitHub Actions owned ARM64', 'github-hosted', ['ubuntu-24.04-arm'])]:
      with self.subTest(name=name):
        value = context_fixture(runner_name=name, runner_environment=environment)
        actual = job_fixture('rust workspace checks', name, labels)
        self.assertEqual(publication.validation_context.verify(value, 42, 1, 'workspace',
            7, SOURCE, 611, runner_job=actual), (611, TREE))
        with self.assertRaisesRegex(ValueError, 'runner differs'):
          publication.validation_context.verify(value, 42, 1, 'workspace', 7, SOURCE,
              611, runner_job=dict(actual, runner_name='another-runner'))

  def test_hosted_receipt_cannot_disguise_self_hosted_job_or_unsupported_labels(self):
    value = context_fixture(runner_name='bin-actions', runner_environment='github-hosted')
    actual = job_fixture('rust workspace checks')
    with self.assertRaisesRegex(ValueError, 'runner environment differs'):
      publication.validation_context.verify(value, 42, 1, 'workspace', 7, SOURCE, 611, runner_job=actual)
    for labels in [['ubuntu-latest'], ['self-hosted', 'ubuntu-24.04'], [], ['Linux', 'X64']]:
      with self.subTest(labels=labels), self.assertRaises(ValueError):
        publication.validation_context.verify(value, 42, 1, 'workspace', 7, SOURCE, 611,
            runner_job=job_fixture('rust workspace checks', 'bin-actions', labels))

  def test_retained_context_cannot_be_rewritten_or_changed_at_job_completion(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      data = encode(context_fixture())
      retain = publication.validation_context.retain
      previous_umask = os.umask(0o077)
      try:
        retain(root, data)
      finally:
        os.umask(previous_umask)
      self.assertEqual((root/'validation-context.json').stat().st_mode & 0o777, 0o444)
      retain(root, data, verify_only=True)
      with self.assertRaises(FileExistsError):
        retain(root, data)
      with self.assertRaisesRegex(ValueError, 'changed during'):
        retain(root, data+b' ', verify_only=True)
      self.assertEqual((root/'validation-context.json').read_bytes(), data)

  def test_original_event_identity_and_complete_commit_bytes_are_required(self):
    value = context_fixture()
    verify = publication.validation_context.verify
    self.assertEqual(verify(value, 42, 1, 'workspace', 7, SOURCE, 611), (611, TREE))
    self.assertNotIn(b'Owned PR body', encode(value))
    for field, changed in [('checkout_sha', 'd'*40), ('checkout_tree', 'd'*40),
                           ('base_sha', 'd'*40), ('commit_object_base64', 'AA=='),
                           ('run_id', 43), ('run_attempt', True), ('job', 'another'),
                           ('pr_number', 612), ('ref', 'refs/heads/dev')]:
      with self.subTest(field=field):
        invalid = dict(value, **{field: changed})
        with self.assertRaises(ValueError):
          verify(invalid, 42, 1, 'workspace', 7, SOURCE, 611)

  def test_release_pr_capture_is_preserved_and_cannot_satisfy_the_dev_publisher(self):
    value = context_fixture(base_ref='main')
    with self.assertRaises(ValueError):
      publication.validation_context.verify(value, 42, 1, 'workspace', 7, SOURCE, 611)
    self.assertEqual(publication.validation_context.verify(value, 42, 1, 'workspace', 7,
        SOURCE, 611, target_base='main'), (611, TREE))

  def test_actual_shallow_git_commit_retains_both_parent_identities(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      command = ['/usr/bin/git', '-c', 'user.name=Owned context fixture', '-c',
          'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', '-C', temp]
      def git(*args, data=None):
        return subprocess.check_output([*command, *args], input=data, stderr=subprocess.PIPE).strip()
      git('init', '-q')
      tree = git('mktree', data=b'').decode()
      base = git('commit-tree', tree, data=b'Owned base\n').decode()
      head = git('commit-tree', tree, '-p', base, data=b'Owned head\n').decode()
      merge = git('commit-tree', tree, '-p', base, '-p', head, data=b'Owned merge\n').decode()
      (root/'.git/shallow').write_text(merge+'\n')
      # Do not strip the raw object: its trailing newline participates in SHA-1.
      raw = subprocess.check_output([*command, 'cat-file', 'commit', merge])
      self.assertEqual(publication.validation_context.commit_object(raw), (merge, tree, [base, head]))

  def test_context_entry_refuses_non_actions_execution_before_reading_git(self):
    with mock.patch.dict(os.environ, {}, clear=True), \
         mock.patch.object(publication.validation_context.subprocess, 'check_output') as git:
      with self.assertRaisesRegex(ValueError, 'registered non-root PR Actions'):
        publication.validation_context.main()
      git.assert_not_called()

  def test_small_api_pinned_proof_is_verified_and_retained_bytes_are_not_replaced(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      fixture = Fixture(root)
      fixture.run['event'] = 'pull_request'
      fixture.run['path'] = '.github/workflows/rust.yml'
      original = root/'owned-proof.zip'
      with zipfile.ZipFile(original, 'w') as archive:
        archive.writestr('validation-context.json', encode(context_fixture()))
      digest = 'sha256:'+hashlib.sha256(original.read_bytes()).hexdigest()
      artifact = dict(id=99, name='native-validation-context-42-1-workspace', expired=False,
          size_in_bytes=original.stat().st_size, digest=digest,
          workflow_run=dict(id=42, head_sha=SOURCE, repository_id=7, head_repository_id=7))
      client = publication.GitHub(root)
      actual_job = job_fixture('rust workspace checks')
      def collect(route, key):
        return [artifact] if key == 'artifacts' else [actual_job]
      with mock.patch.object(client, 'collect', side_effect=collect), \
           mock.patch.object(client, 'download', side_effect=lambda route, pin, path:shutil.copyfile(original, path)) as download:
        context, proof = client.pr_context(fixture.run, SOURCE, 611, 'workspace')
        self.assertEqual(context, {(611, TREE)})
        self.assertEqual(proof['artifact_digest'], digest)
        actual_job['runner_name'] = 'bin-actions2'
        with self.assertRaisesRegex(ValueError, 'runner differs'):
          client.pr_context(fixture.run, SOURCE, 611, 'workspace')
        actual_job['runner_name'] = 'bin-actions'
        # GitHub's live linked PR fields may move after the original event.
        fixture.run['pull_requests'] = [dict(number=611, head=dict(sha='e'*40), base=dict(sha='f'*40))]
        self.assertEqual(client.pr_context(fixture.run, SOURCE, 611, 'workspace')[0], context)
        self.assertEqual(download.call_count, 1)
        retained = root/'validation-contexts/42-1-workspace.zip'
        raw = retained.read_bytes()
        retained.write_bytes(b'X'+raw[1:])
        with self.assertRaisesRegex(ValueError, 'independent artifact pin'):
          client.pr_context(fixture.run, SOURCE, 611, 'workspace')
        self.assertEqual(download.call_count, 1)
        retained.unlink()
        retained.symlink_to(original)
        with self.assertRaisesRegex(ValueError, 'redirected'):
          client.pr_context(fixture.run, SOURCE, 611, 'workspace')
        self.assertEqual(original.read_bytes(), raw)
        with self.assertRaisesRegex(ValueError, 'another event type'):
          client.pr_context(dict(fixture.run, event='push'), SOURCE, 611, 'workspace')
      for field in ('head_sha', 'repository_id', 'head_repository_id'):
        with self.subTest(field=field):
          invalid = copy.deepcopy(artifact)
          invalid['workflow_run'][field] = 'f'*40 if field == 'head_sha' else 8
          with self.assertRaisesRegex(ValueError, 'artifact identity'):
            publication.artifact_pin(invalid, fixture.run, SOURCE, 1024**2)


class GateTests(unittest.TestCase):
  def fixture(self):
    producer = dict(id=42, run_attempt=1, head_sha=SOURCE, path=publication.PRODUCER, event='push',
        repository=dict(full_name=publication.REPO, id=7), head_repository=dict(full_name=publication.REPO),
        status='completed', conclusion='success')
    runs, proofs, jobs = [], {}, {42:[job_fixture('Native compiler in pinned AGNOS ARM userspace')]}
    for number, (name, events, required) in enumerate(publication.GATES, 100):
      run = dict(producer, id=number, path='.github/workflows/'+name, event=events[0])
      if run['event'] == 'pull_request':
        repository = dict(id=7, url='https://api.github.com/repos/'+publication.REPO)
        run['pull_requests'] = [dict(number=611,
            base=dict(ref='dev', sha=TREE, repo=repository.copy()),
            head=dict(ref='codex/owned-source', sha=SOURCE, repo=repository.copy()))]
        proofs[number] = context_fixture(number, publication.PROOF_JOBS[name])
      runs.append(run)
      jobs[number] = [job_fixture(n) for n in required]
    client = mock.Mock()
    client.json.return_value = producer
    def collect(route, key):
      return runs if key == 'workflow_runs' else jobs[int(route.split('/')[2])]
    client.collect.side_effect = collect
    def pr_context(run, source, number, job):
      value = proofs[run['id']]
      actual = [row for row in jobs[run['id']] if row['name'] == publication.PROOF_JOB_NAMES[job]]
      if len(actual) != 1:
        raise ValueError('owned fixture needs one actual proof job')
      context = publication.validation_context.verify(value, run['id'], run['run_attempt'], job,
          run['repository']['id'], source, number, runner_job=actual[0])
      return {context}, dict(pr_number=context[0], base_sha=context[1])
    client.pr_context.side_effect = pr_context
    return client, producer, runs, jobs, proofs

  def test_latest_pending_wrong_runner_and_missing_required_arm_job_block_publication(self):
    client, producer, runs, jobs, _ = self.fixture()
    self.assertTrue(all(s['ready'] for s in publication.gates(client, SOURCE, 42, 611)[1]))
    runs.append(dict(runs[-1], id=300, status='queued', conclusion=None))
    self.assertFalse(publication.gates(client, SOURCE, 42, 611)[1][-1]['ready'])
    runs.pop()
    jobs[100] = [job for job in jobs[100] if job['name'] != 'Actual offline Cargo reuse (ARM QEMU)']
    self.assertFalse(publication.gates(client, SOURCE, 42, 611)[1][0]['ready'])
    jobs[101][0]['runner_name'] = 'unregistered-hosted-runner'
    self.assertFalse(publication.gates(client, SOURCE, 42, 611)[1][1]['ready'])
    client.json.return_value = dict(producer, conclusion='failure')
    with self.assertRaisesRegex(ValueError, 'completed successfully'):
      publication.gates(client, SOURCE, 42, 611)

  def test_general_gates_allow_hosted_or_second_registered_runner_with_bound_receipts(self):
    for name, environment, labels in [
        ('bin-actions2', 'self-hosted', ['self-hosted', 'Linux', 'X64']),
        ('bin-actions3', 'self-hosted', ['self-hosted', 'Linux', 'X64']),
        ('GitHub Actions owned X64', 'github-hosted', ['ubuntu-24.04']),
        ('GitHub Actions owned ARM64', 'github-hosted', ['ubuntu-24.04-arm'])]:
      with self.subTest(name=name):
        client, _, runs, jobs, proofs = self.fixture()
        for run in runs:
          filename = run['path'].rsplit('/', 1)[-1]
          if filename in publication.SELF_HOSTED_GATES:
            continue
          for actual in jobs[run['id']]:
            actual.update(runner_name=name, labels=labels)
          if run['event'] == 'pull_request':
            proofs[run['id']] = context_fixture(run['id'], publication.PROOF_JOBS[filename],
                runner_name=name, runner_environment=environment)
        self.assertTrue(all(state['ready'] for state in publication.gates(client, SOURCE, 42, 611)[1]))

  def test_native_gates_and_selected_producer_keep_the_self_hosted_boundary(self):
    client, _, _, jobs, _ = self.fixture()
    for rows in jobs.values():
      for actual in rows:
        actual['runner_name'] = 'bin-actions2'
    # Keep unchanged original PR receipts attached to their actual runner.
    for number in (103, 104, 106, 107):
      for actual in jobs[number]:
        actual['runner_name'] = 'bin-actions'
    self.assertTrue(all(state['ready'] for state in publication.gates(client, SOURCE, 42, 611)[1]))
    jobs[100][0].update(runner_name='GitHub Actions owned', labels=['ubuntu-24.04'])
    self.assertFalse(publication.gates(client, SOURCE, 42, 611)[1][0]['ready'])
    jobs[42][0].update(runner_name='GitHub Actions owned', labels=['ubuntu-24.04'])
    with self.assertRaisesRegex(ValueError, 'registered-runner job'):
      publication.gates(client, SOURCE, 42, 611)

  def test_unregistered_self_hosted_and_mislabelled_jobs_never_satisfy_general_gates(self):
    for name, labels in [('unregistered-actions', ['self-hosted', 'Linux', 'X64']),
                         ('bin-actions2', ['self-hosted', 'Linux']),
                         ('GitHub Actions owned', ['self-hosted', 'ubuntu-24.04'])]:
      with self.subTest(name=name, labels=labels):
        client, _, _, jobs, _ = self.fixture()
        jobs[105][0].update(runner_name=name, labels=labels)
        self.assertFalse(publication.gates(client, SOURCE, 42, 611)[1][5]['ready'])

  def test_publication_rejects_receipt_runner_name_and_environment_spoofing(self):
    for fault in ('name', 'environment'):
      with self.subTest(fault=fault):
        client, _, _, jobs, proofs = self.fixture()
        if fault == 'name':
          next(row for row in jobs[103] if row['name'] == 'rust workspace checks')['runner_name'] = 'bin-actions2'
        else:
          proofs[103]['runner_environment'] = 'github-hosted'
        with self.assertRaisesRegex(ValueError, 'runner'):
          publication.gates(client, SOURCE, 42, 611)

  def test_publisher_execution_rejects_hosted_and_other_self_hosted_before_api_access(self):
    argv = ['publish_inputs.py', '--source-run-id', '42', '--source-pr-number', '611',
        '--source-commit', SOURCE, '--index-sha256', 'a'*64, '--publish']
    for name, environment in [('GitHub Actions owned', 'github-hosted'), ('unregistered-actions', 'self-hosted')]:
      env = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT=environment, RUNNER_NAME=name,
          RUNNER_OS='Linux', RUNNER_ARCH='X64', GITHUB_REPOSITORY=publication.REPO,
          GITHUB_SERVER_URL='https://github.com', GITHUB_EVENT_NAME='workflow_dispatch')
      with self.subTest(name=name), mock.patch.dict(os.environ, env, clear=True), \
           mock.patch.object(publication.sys, 'argv', argv), \
           mock.patch.object(publication.sys, 'flags', mock.Mock(isolated=1, no_site=1, dont_write_bytecode=1)), \
           mock.patch.object(publication, 'GitHub') as api:
        with self.assertRaisesRegex(ValueError, 'registered manual Actions'):
          publication.main()
        api.assert_not_called()

  def test_pr_gates_require_one_exact_source_dev_pr_and_base_context(self):
    for fault in ('base', 'head', 'base-repository', 'head-repository', 'number', 'attempt'):
      with self.subTest(fault=fault):
        client, _, _, _, proofs = self.fixture()
        proof = proofs[107]
        if fault == 'base': proof['base_ref'] = 'dev-md'
        if fault == 'head': proof['head_sha'] = 'f'*40
        if fault == 'base-repository': proof['base_repository_id'] = 8
        if fault == 'head-repository': proof['head_repository_id'] = 8
        if fault == 'number': proof['pr_number'] = True
        if fault == 'attempt': proof['run_attempt'] = 2
        with self.assertRaises(ValueError):
          publication.gates(client, SOURCE, 42, 611)

  def test_missing_context_or_mixed_original_bases_never_form_one_green_candidate(self):
    for fault in ('missing', 'different-base'):
      with self.subTest(fault=fault):
        client, _, runs, _, proofs = self.fixture()
        if fault == 'missing': runs[-1]['pull_requests'] = []
        else: proofs[107] = context_fixture(107, 'check_user_docs', 'e'*40)
        states = publication.gates(client, SOURCE, 42, 611)[1]
        self.assertFalse(states[-1]['ready'])

  def test_mutable_associated_pr_head_and_base_do_not_rewrite_the_original_event(self):
    client, _, runs, _, _ = self.fixture()
    for run in runs:
      for pull in run.get('pull_requests', []):
        pull['head']['sha'] = 'f'*40
        pull['base']['sha'] = 'e'*40
        pull['base']['ref'] = 'main'
    states = publication.gates(client, SOURCE, 42, 611)[1]
    self.assertTrue(all(state['ready'] for state in states))
    self.assertEqual(states[-1]['pull_requests'], [dict(number=611, base_sha=TREE)])

  def test_an_unrelated_pr_number_does_not_replace_the_requested_pr_run(self):
    client, _, runs, _, _ = self.fixture()
    unrelated = copy.deepcopy(runs[-1])
    unrelated['id'] = 300
    unrelated['pull_requests'][0]['number'] = 612
    runs.append(unrelated)
    states = publication.gates(client, SOURCE, 42, 611)[1]
    self.assertTrue(all(state['ready'] for state in states))
    self.assertEqual(states[-1]['run'], 107)
    self.assertEqual(states[-1]['pull_requests'], [dict(number=611, base_sha=TREE)])


if __name__ == '__main__':
  unittest.main()

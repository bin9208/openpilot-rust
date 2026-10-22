"""Exercise the actual delivery admission script with owned GitHub responses."""
import ast
import copy
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

import yaml

ROOT = Path(__file__).resolve().parents[3]
SHA = 'a' * 40
GATES = ['fast checks', 'check mapped user docs', 'integration gate', 'rust checks', 'rust aarch64 build']


def fixture():
  run = dict(id=42, run_attempt=1, path='.github/workflows/ci.yml', head_sha=SHA,
             head_branch='dev-md', event='push', repository=dict(full_name='bin9208/openpilot-rust'),
             status='completed', conclusion='success')
  return dict(sha=SHA, run_id='42', branch_sha=SHA, message='Merge PR [runtime-package]',
              prs=[dict(merged_at='2026-10-20T00:00:00Z', merge_commit_sha=SHA, base=dict(ref='dev-md'))],
              run=run, latest=copy.deepcopy(run),
              jobs=[dict(name=name, head_sha=SHA, status='completed', conclusion='success') for name in GATES])


def invoke(values):
  spec = yaml.load((ROOT / '.github/workflows/dev-md-runtime.yml').read_text(), Loader=yaml.BaseLoader)
  script = spec['jobs']['verify']['steps'][0]['with']['script']
  harness = '''
const f = JSON.parse(process.env.FIXTURE);
process.env.SOURCE_SHA=f.sha; process.env.CI_RUN_ID=f.run_id;
const context={repo:{owner:'bin9208',repo:'openpilot-rust'}};
const github={rest:{repos:{getBranch:async()=>({data:{commit:{sha:f.branch_sha}}}),
getCommit:async()=>({data:{commit:{message:f.message}}}),listPullRequestsAssociatedWithCommit:'prs'},
actions:{getWorkflowRun:async()=>({data:f.run}),listWorkflowRunsForRepo:'latest',listJobsForWorkflowRun:'jobs'}},
paginate:async(method)=>method==='prs'?f.prs:method==='latest'?[f.latest]:f.jobs};
const core={setOutput:(key,value)=>process.stdout.write(JSON.stringify({[key]:value}))};
const setTimeout=callback=>callback();
(async()=>{ SCRIPT })().catch(e=>{console.error(e.message);process.exit(1)});
'''.replace('SCRIPT', script)
  return subprocess.run(['node', '-e', harness], env={**os.environ, 'FIXTURE': json.dumps(values)},
                        capture_output=True, text=True)


class RuntimeDeliveryTests(unittest.TestCase):
  def test_inherited_gpu_provider_checks_stay_in_the_common_serial_path(self):
    ci = yaml.load((ROOT / '.github/workflows/ci.yml').read_text(), Loader=yaml.BaseLoader)['jobs']
    native = ci['xiaoge-runtime']
    self.assertEqual(native['env']['OPENCV_TEST_GPU'], 'reject')
    scripts = [step.get('run', '') for step in native['steps']]
    self.assertTrue(any('--bin build-xiaoge-opencv -- --opencl' in script for script in scripts))
    self.assertTrue(any('--example xiaoge_compute_bench' in script and '-- --assert' in script for script in scripts))
    self.assertTrue(any('python3 rust/ci/runtime/test_package_policy.py' in step.get('run', '')
                        for step in ci['fast-checks']['steps']))

  def test_ci_only_dm_fixture_does_not_change_reused_producers(self):
    tree = ast.parse((ROOT / 'rust/ci/runtime/package.py').read_text())
    policy = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'source_path_allowed')
    namespace = {}
    exec(compile(ast.Module(body=[policy], type_ignores=[]), '<source-policy>', 'exec'), namespace)
    admitted = namespace['source_path_allowed']
    self.assertTrue(admitted('rust/tools/check_dmonitoring_daemon.py'))
    self.assertTrue(admitted('rust/tools/check_card_runtime_pumped.py'))
    self.assertTrue(admitted('tools/ci/tests/test_card_warmup.py'))
    for producer in ('rust/tools/build_model_tinygrad.py', 'openpilot/selfdrive/modeld/models/driving_vision.onnx',
                     'third_party/ffmpeg/configure', 'rust/tools/check_dmonitoring_daemon.py.unreviewed'):
      self.assertFalse(admitted(producer), producer)

  def test_inherited_document_and_unknown_source_guards_survive_ci_consolidation(self):
    tree = ast.parse((ROOT / 'rust/ci/runtime/package.py').read_text())
    policy = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'source_path_allowed')
    namespace = {}
    exec(compile(ast.Module(body=[policy], type_ignores=[]), '<source-policy>', 'exec'), namespace)
    allowed = namespace['source_path_allowed']
    path = 'docs/naver/self-hosted-runner-404.md'
    entry = '100644 blob original\t' + path
    self.assertTrue(allowed(path, entry, entry))
    for changed in ('', entry.replace('original', 'changed'), entry.replace('100644', '100755'), entry.replace('blob', 'commit')):
      self.assertFalse(allowed(path, changed, entry))
    for unknown in ('.github/workflows/unrelated.yml', 'tools/ci/tests/test_other.py', '.lfsconfig.extra',
                    '.github/workflows/ci.yml.extra', 'rust/crates/opencv-runtime/native/image.cc.extra',
                    'rust/ci/runtime/unrelated.py', 'rust/tools/tests/test_unreviewed.py'):
      self.assertFalse(allowed(unknown, 'same', 'same'), unknown)
    for reviewed in ('.github/workflows/ci.yml', '.github/workflows/rust.yml', '.lfsconfig',
                     'rust/crates/xiaoge/tests/cadence.rs', 'tools/ci/tests/test_lfs_endpoint.py',
                     'tools/ci/tests/test_eintr_cleanup.py', '.github/actions/job-temp/action.yml',
                     'rust/crates/opencv-runtime/src/lib.rs', 'rust/tools/tests/test_card_ci.py'):
      self.assertTrue(allowed(reviewed), reviewed)

  def test_completed_latest_exact_merge_is_admitted(self):
    result = invoke(fixture())
    self.assertEqual(result.returncode, 0, result.stderr)
    self.assertEqual(json.loads(result.stdout), dict(source_sha=SHA))

  def test_wrong_or_missing_evidence_is_rejected_before_build(self):
    faults = ['sha', 'run-id', 'stale-branch', 'marker', 'no-pr', 'unmerged', 'wrong-merge',
              'wrong-base', 'workflow', 'head', 'branch', 'event', 'repository', 'running',
              'failed', 'cancelled', 'superseded', 'rerun', 'missing-job', 'duplicate-job',
              'job-head', 'job-skipped', 'job-cancelled']
    for fault in faults:
      f = fixture()
      if fault == 'sha': f['sha'] = 'invalid'
      elif fault == 'run-id': f['run_id'] = 'invalid'
      elif fault == 'stale-branch': f['branch_sha'] = 'b' * 40
      elif fault == 'marker': f['message'] = 'ordinary CI merge'
      elif fault == 'no-pr': f['prs'] = []
      elif fault == 'unmerged': f['prs'][0]['merged_at'] = None
      elif fault == 'wrong-merge': f['prs'][0]['merge_commit_sha'] = 'b' * 40
      elif fault == 'wrong-base': f['prs'][0]['base']['ref'] = 'dev'
      elif fault == 'workflow': f['run']['path'] = '.github/workflows/rust.yml'
      elif fault == 'head': f['run']['head_sha'] = 'b' * 40
      elif fault == 'branch': f['run']['head_branch'] = 'dev'
      elif fault == 'event': f['run']['event'] = 'pull_request'
      elif fault == 'repository': f['run']['repository']['full_name'] = 'someone/else'
      elif fault == 'running': f['run']['status'] = 'in_progress'
      elif fault == 'failed': f['run']['conclusion'] = 'failure'
      elif fault == 'cancelled': f['run']['conclusion'] = 'cancelled'
      elif fault == 'superseded': f['latest']['id'] = 43
      elif fault == 'rerun': f['latest']['run_attempt'] = 2
      elif fault == 'missing-job': f['jobs'].pop()
      elif fault == 'duplicate-job': f['jobs'].append(f['jobs'][0])
      elif fault == 'job-head': f['jobs'][0]['head_sha'] = 'b' * 40
      elif fault == 'job-skipped': f['jobs'][0]['conclusion'] = 'skipped'
      elif fault == 'job-cancelled': f['jobs'][0]['conclusion'] = 'cancelled'
      with self.subTest(fault=fault):
        result = invoke(f)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(result.stdout, '')


class RuntimePackageRunnerTests(unittest.TestCase):
  @classmethod
  def setUpClass(cls):
    source = ROOT / 'rust/ci/runtime/package.py'
    tree = ast.parse(source.read_text())
    # Execute the real entry admission without initializing work directories,
    # reading Git state or invoking any build/delivery command.
    entry = []
    for node in tree.body:
      if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'ROOT' for target in node.targets):
        break
      entry.append(node)
    cls.entry = compile(ast.Module(body=entry, type_ignores=[]), str(source), 'exec')
    cls.namespace = dict(os=os, json=json, hashlib=hashlib, shlex=shlex, shutil=shutil, Path=Path, WORK=ROOT)
    functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in {'save', 'save_receipt', 'room', 'digest'}]
    exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), 'exec'), cls.namespace)
    build = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'build')
    receipt = next(node for node in build.body if isinstance(node, ast.Expr) and isinstance(node.value, ast.Call)
                   and node.value.args and any(isinstance(part, ast.Constant) and part.value == 'installer-build.json'
                                               for part in ast.walk(node.value.args[0])))
    cls.installer_receipt = compile(ast.Module(body=[receipt], type_ignores=[]), str(source), 'exec')

  def environment(self):
    return dict(GITHUB_ACTIONS='true', GITHUB_REPOSITORY='bin9208/openpilot-rust',
                RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='bin-actions')

  def test_real_self_hosted_actions_admission_preserves_environment(self):
    environment = self.environment()
    with mock.patch.dict(os.environ, environment, clear=True):
      exec(self.entry, {})
      self.assertEqual(dict(os.environ), environment)

  def test_hosted_workstation_unknown_runner_and_wrong_repository_are_rejected(self):
    faults = (
      ('GITHUB_ACTIONS', None), ('GITHUB_ACTIONS', 'false'),
      ('GITHUB_REPOSITORY', None), ('GITHUB_REPOSITORY', 'other/openpilot-rust'),
      ('RUNNER_ENVIRONMENT', None), ('RUNNER_ENVIRONMENT', 'github-hosted'),
      ('RUNNER_ENVIRONMENT', ''), ('RUNNER_NAME', None), ('RUNNER_NAME', 'other-runner'),
    )
    for key, value in faults:
      environment = self.environment()
      if value is None:
        environment.pop(key)
      else:
        environment[key] = value
      with self.subTest(key=key, value=value), mock.patch.dict(os.environ, environment, clear=True):
        with self.assertRaises(AssertionError):
          exec(self.entry, {})
        self.assertEqual(dict(os.environ), environment)

  def test_delivery_receipts_record_actual_runner_without_relabelling(self):
    with tempfile.TemporaryDirectory() as temp, mock.patch.dict(os.environ, self.environment(), clear=True):
      receipt = Path(temp) / 'installer-build.json'
      installer = Path(temp) / 'installer'
      installer.write_bytes(b'test')
      namespace = dict(self.namespace, WORK=Path(temp), SHA=SHA, installer=installer, installer_cmd=['cargo', 'build'])
      # Run the actual build receipt emission, without invoking the build.
      exec(self.installer_receipt, namespace)
      actual = json.loads(receipt.read_text())
      self.assertEqual(actual['source_commit'], SHA)
      self.assertEqual(actual['installer_sha256'], '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08')
      self.assertEqual(actual['runner_environment'], 'self-hosted')
      self.assertEqual(actual['runner_name'], 'bin-actions')
      self.assertEqual(os.environ['RUNNER_ENVIRONMENT'], 'self-hosted')
      self.assertEqual(os.environ['RUNNER_NAME'], 'bin-actions')

  def test_receipt_provenance_comes_from_environment_instead_of_existing_fields(self):
    environment = dict(self.environment(), RUNNER_NAME='receipt-runner-fixture')
    with tempfile.TemporaryDirectory() as temp, mock.patch.dict(os.environ, environment, clear=True):
      receipt = Path(temp) / 'receipt.json'
      value = dict(runner_environment='github-hosted', runner_name='stale-runner')
      self.namespace['save_receipt'](receipt, value)
      self.assertEqual(json.loads(receipt.read_text()), dict(runner_environment='self-hosted', runner_name='receipt-runner-fixture'))
      self.assertEqual(value, dict(runner_environment='github-hosted', runner_name='stale-runner'))

  def test_room_admits_estimated_growth_without_fixed_reserve(self):
    for free, growth in ((0, 0), (2 * 2**30, 2**30), (20 * 2**30, 20 * 2**30)):
      with self.subTest(free=free, growth=growth), mock.patch.object(shutil, 'disk_usage', return_value=SimpleNamespace(free=free)):
        if growth == 0:
          self.namespace['room']()
        else:
          self.namespace['room'](growth)

  def test_room_still_rejects_insufficient_estimated_growth(self):
    for free, growth in ((0, 1), (20 * 2**30 - 1, 20 * 2**30)):
      with self.subTest(free=free, growth=growth), mock.patch.object(shutil, 'disk_usage', return_value=SimpleNamespace(free=free)):
        with self.assertRaisesRegex(AssertionError, f'free={free}, growth={growth}'):
          self.namespace['room'](growth)


if __name__ == '__main__':
  unittest.main()

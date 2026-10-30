"""Exercise the actual delivery admission script with owned GitHub responses."""
import ast
import copy
import json
import os
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
SHA = 'a' * 40
GATES = ['check mapped user docs', 'integration gate', 'rust checks', 'rust aarch64 build']


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
                        for step in ci['check_user_docs']['steps']))

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
                    '.github/workflows/ci.yml.extra', 'rust/crates/opencv-runtime/native/image.cc',
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


if __name__ == '__main__':
  unittest.main()

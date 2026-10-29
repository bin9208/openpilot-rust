"""Retain exact required commands while routing this track to the named runner."""
from collections import Counter
import hashlib
import json
import runpy
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = 'e445c9d19c3041bc79d8624589e2eb6ba7d061c6'
LABELS = ['self-hosted', 'Linux', 'X64']
INHERITED_WORKFLOWS = {'carrot-route-vault-publish.yaml', 'naver-upstream-sync.yml', 'sync.yml', 'wiki-settings-publish.yaml'}
HOSTED_JOBS = {'fast-checks.yml': {'fast'}, 'user-docs.yaml': {'check_user_docs'}, 'integration.yml': {'gate'}, 'rust.yml': {'fast'}, 'wiki-settings-check.yaml': {'check'}}
SAFETY = "github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository"
HOSTED_CLEANUP = 'sudo rm -rf /usr/local/lib/android /opt/ghc /usr/share/dotnet'


def load(path):
    return yaml.load(Path(path).read_text(), Loader=yaml.BaseLoader)


def normalize(script):
    # Only these two source-reviewed changes may alter an inherited command.
    script = ''.join(line for line in script.splitlines(keepends=True)
                     if line.strip() != HOSTED_CLEANUP)
    return script.replace('(25 + 2) * 1024 * 1024 * 1024', '2 * 1024 * 1024 * 1024').replace(
        'Retain 25 GiB plus build growth; recover at least 35 GiB before retry.',
        'Insufficient disk space for estimated documentation build growth (2 GiB).')


def digest(script):
    return hashlib.sha256(script.encode()).hexdigest()


def condition(original):
    expression = original.strip()
    if expression.startswith('${{') and expression.endswith('}}'):
        expression = expression[3:-2].strip()
    return '${{ (' + SAFETY + ')' + (' && (' + expression + ')' if expression else '') + ' }}'


class LocalNativeWorkflowTests(unittest.TestCase):
    def test_source_build_uses_real_user_permissions_after_root_only_package_bootstrap(self):
        job = load(ROOT/'.github/workflows/runtime-source-build.yml')['jobs']['offline-cargo-reuse']
        self.assertEqual(job['env']['LOCAL_CI_GUEST_IDENTITY'], 'unprivileged')
        self.assertTrue(any('test_readonly_publication.py' in step.get('run', '') for step in job['steps']))
        transport = (ROOT/'tools/ci/local-arm').read_text()
        self.assertIn('LOCAL_CI_GUEST_IDENTITY=root guest /bin/bash', transport)
        self.assertIn("unprivileged) ;;", transport)
        self.assertIn('guest_identity', transport)
        self.assertIn('guest_uid', transport)

    def test_validation_concurrency_preserves_pending_and_running_evidence(self):
        for name in ('fast-checks.yml', 'integration.yml', 'rust.yml', 'tests.yaml',
                     'naver-ci.yml', 'naver-apk-tools.yml', 'wiki-settings-check.yaml',
                     'native-apt-scope.yml', 'carrot-peer-readiness.yml'):
            with self.subTest(workflow=name):
                policy = load(ROOT / '.github/workflows' / name)['concurrency']
                self.assertEqual(policy['queue'], 'max')
                self.assertEqual(policy['cancel-in-progress'], 'false')
                self.assertTrue(policy['group'].endswith('-parallel-v2'))

    def test_every_execution_job_requires_the_registered_local_labels(self):
        for path in sorted((ROOT / '.github/workflows').glob('*')):
            if path.suffix not in ('.yml', '.yaml'):
                continue
            document = load(path)
            for name, job in document.get('jobs', {}).items():
                if 'runs-on' in job:
                    with self.subTest(path=path.name, job=name):
                        expected = (LABELS + ['dev-md-local'] if path.name in INHERITED_WORKFLOWS else
                                    'ubuntu-24.04' if name in HOSTED_JOBS.get(path.name, set()) else LABELS)
                        self.assertEqual(job['runs-on'], expected)
                        self.assertIn(SAFETY, job['if'])
        template = load(ROOT / 'rust/crates/carrot-man/ci-jobs.yml')['carrot-man-runtime']
        self.assertEqual(template['runs-on'], LABELS)
        owner = (ROOT / '.github/actions/job-temp/action.yml').read_text()
        self.assertIn('runner_policy.py', owner)
        policy = runpy.run_path(str(ROOT/'tools/ci/runner_policy.py'))
        for runner in ('bin-actions', 'bin-actions2', 'bin-actions3'):
            self.assertTrue(policy['environment_allowed'](dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME=runner), self_hosted_only=True))
            self.assertTrue(policy['job_is_allowed'](dict(runner_name=runner, labels=LABELS), self_hosted_only=True))
        self.assertFalse(policy['environment_allowed'](dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='other-session')))
        self.assertTrue(policy['environment_allowed'](dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted', RUNNER_OS='Linux', RUNNER_ARCH='X64')))
        self.assertIn('--self-hosted-only', (ROOT/'tools/ci/local-arm').read_text())

    def test_inherited_sync_and_publish_workflows_keep_the_exact_pre_port_bytes(self):
        for name in INHERITED_WORKFLOWS:
            relative = '.github/workflows/'+name
            with self.subTest(workflow=name):
                original = subprocess.check_output(['git', 'show',
                    'ec418df39d3b5820f2751978e3898f0a4d858cbc:'+relative], cwd=ROOT)
                self.assertEqual((ROOT/relative).read_bytes(), original)

    def test_frozen_jobs_dependencies_matrix_and_each_original_command_survive(self):
        snapshot = json.loads((ROOT / 'tools/ci/fixtures/local-native-workflows-605.json').read_text())
        self.assertEqual(snapshot['schema'], 1)
        self.assertEqual(snapshot['source_commit'], BASE)
        for relative, frozen in snapshot['workflows'].items():
            original = yaml.load(subprocess.check_output(['git', 'show', BASE + ':' + relative],
                                                        cwd=ROOT, text=True), Loader=yaml.BaseLoader)
            current = load(ROOT / relative)
            self.assertEqual(set(current['jobs']), set(original['jobs']))
            for name, before in original['jobs'].items():
                with self.subTest(path=relative, job=name):
                    after = current['jobs'][name]
                    self.assertEqual(after.get('needs'), before.get('needs'))
                    self.assertEqual(after.get('strategy', {}).get('matrix'), before.get('strategy', {}).get('matrix'))
                    expected = [digest(normalize(step['run'])) for step in before.get('steps', []) if 'run' in step]
                    self.assertEqual(frozen['jobs'][name]['scripts'], expected)
                    actual = Counter(digest(step['run']) for step in after.get('steps', []) if 'run' in step)
                    self.assertFalse(Counter(expected) - actual, 'original command changed or removed')
                    before_steps = Counter((digest(normalize(s['run'])), s.get('if'), s.get('continue-on-error'))
                                           for s in before.get('steps', []) if 'run' in s)
                    after_steps = Counter((digest(s['run']), s.get('if'), s.get('continue-on-error'))
                                          for s in after.get('steps', []) if 'run' in s)
                    self.assertFalse(before_steps - after_steps, 'original command gained an escape/skip')
                    if 'runs-on' in before:
                        self.assertEqual(after['if'], condition(before.get('if', '')))
                    for event, config in original.get('on', {}).items():
                        self.assertIn(event, current['on'])
                        if isinstance(config, dict) and 'branches' in config:
                            self.assertTrue(set(config['branches']).issubset(current['on'][event]['branches']))

    def test_arm_variants_use_the_separate_guest_and_preserve_evidence(self):
        snapshot = json.loads((ROOT / 'tools/ci/fixtures/local-native-workflows-605.json').read_text())
        for relative in snapshot['workflows']:
            original = yaml.load(subprocess.check_output(['git', 'show', BASE + ':' + relative],
                                                        cwd=ROOT, text=True), Loader=yaml.BaseLoader)
            current = load(ROOT / relative)
            for name, before in original['jobs'].items():
                runner = before.get('runs-on', '')
                if runner != 'ubuntu-24.04-arm' and 'matrix.runner' not in runner:
                    continue
                with self.subTest(path=relative, job=name):
                    job = current['jobs'][name]
                    timeout = job['timeout-minutes']
                    if runner == 'ubuntu-24.04-arm':
                        self.assertGreaterEqual(int(timeout), 90)
                    else:
                        prefix = "${{ matrix.runner == 'ubuntu-24.04-arm' && "
                        suffix = ' || ' + before['timeout-minutes'] + ' }}'
                        self.assertTrue(timeout.startswith(prefix))
                        self.assertTrue(timeout.endswith(suffix), 'X64 time budget changed')
                        self.assertGreaterEqual(int(timeout[len(prefix):-len(suffix)]), 90)
                    self.assertEqual(job['defaults']['run']['shell'], 'openpilot-local-shell {0}')
                    self.assertIn('LOCAL_CI_ARCH', job['env'])
                    steps = job['steps']
                    self.assertTrue(any(s.get('uses') == './.github/actions/local-arm' and
                                        s.get('with', {}).get('cleanup') != 'true' for s in steps))
                    self.assertTrue(any(s.get('uses', '').startswith('actions/upload-artifact@') and
                                        'local-arm-' in s.get('with', {}).get('name', '') and
                                        'CI_JOB_TEMP' in s['with']['path'] for s in steps))
                    self.assertTrue(any(s.get('uses') == './.github/actions/job-temp' and
                                        s.get('with', {}).get('cleanup') == 'true' and
                                        s.get('if') == 'always()' for s in steps))
        transport = (ROOT / 'tools/ci/local-arm').read_text()
        for proof in ('guest_machine == aarch64', '"physical_arm_cpu":false',
                      'host_architecture', 'guest_architecture', 'proot_sha256', 'rootfs_sha256'):
            self.assertIn(proof, transport)

    def test_shared_system_tools_are_not_deleted_for_hosted_image_space(self):
        for path in (ROOT / '.github/workflows').glob('*'):
            if path.suffix in ('.yml', '.yaml'):
                self.assertNotIn(HOSTED_CLEANUP, path.read_text())
        self.assertNotIn(HOSTED_CLEANUP, (ROOT / 'rust/crates/carrot-man/ci-jobs.yml').read_text())


if __name__ == '__main__':
    unittest.main()

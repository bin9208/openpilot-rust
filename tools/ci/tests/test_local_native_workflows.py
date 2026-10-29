"""Retain exact required commands while routing this track to the named runner."""
from collections import Counter
from functools import cache
import hashlib
import json
import runpy
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = 'e445c9d19c3041bc79d8624589e2eb6ba7d061c6'
SUPPORT_BASE = '3fd7fd051b0af237fb8f75a4260e48707e3ae7c8'
LABELS = ['self-hosted', 'Linux', 'X64', 'openpilot-rust']
INHERITED_WORKFLOWS = {'carrot-route-vault-publish.yaml', 'naver-upstream-sync.yml', 'sync.yml', 'wiki-settings-publish.yaml'}
HOSTED_JOBS = {'fast-checks.yml': {'fast'}, 'user-docs.yaml': {'check_user_docs'}, 'integration.yml': {'gate'},
    'rust.yml': {'fast', 'support-runtime', 'telemetry-runtime', 'web-upload-timeouts', 'joystickd-runtime',
        'encoder-runtime', 'camera-runtime', 'panda-runtime', 'selfdrive-runtime', 'radar-runtime',
        'navd-runtime', 'carrot-navi-runtime', 'startup-runtime', 'startup-services', 'sensor-audio',
        'gnss-runtime', 'controls-runtime', 'estimation-runtime', 'ui-connectivity', 'platform-runtime',
        'hardware-runtime', 'logger-runtime', 'athena-runtime'},
    'wiki-settings-check.yaml': {'check'}, 'jetlink.yml': {'android-core'}}
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


def parallel_defaults(script):
    # User-authorized removal of resource caps only. The frozen command hashes
    # are still checked separately against the original source bytes below.
    return script.replace('CARGO_BUILD_JOBS=2 ', '').replace(' -- --test-threads=1', '').replace(' --test-threads=1', '')


def restore_parallel_caps(script, originals):
    for original in originals:
        if script == parallel_defaults(original):
            return original
    return script


@cache
def support_workspace_command_pairs():
    # Compare only the two reviewed #631 scripts with their exact frozen bytes.
    commands = (
        ('cargo clippy --workspace --all-targets --locked -- -D warnings',
         'cargo clippy --workspace --all-targets --locked --features openpilot-modem/helper-lifetime-tests -- -D warnings'),
        ('cargo test --workspace --locked',
         'cargo test --workspace --locked --features openpilot-modem/helper-lifetime-tests'),
    )
    versions = []
    for ref, column in ((BASE, 0), (SUPPORT_BASE, 1)):
        document = yaml.load(subprocess.check_output(
            ['git', 'show', ref + ':.github/workflows/rust.yml'], cwd=ROOT, text=True),
            Loader=yaml.BaseLoader)
        scripts = []
        for pair in commands:
            script, = (step['run'] for step in document['jobs']['workspace']['steps']
                       if pair[column] in step.get('run', '').splitlines())
            scripts.append(script)
        versions.append(scripts)
    return tuple(zip(*versions))


def restore_support_workspace_command(relative, name, script):
    if relative == '.github/workflows/rust.yml' and name == 'workspace':
        for original, updated in support_workspace_command_pairs():
            if script == updated:
                return original
    return script


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
                        expected = (['self-hosted', 'Linux', 'X64', 'dev-md-local'] if path.name in INHERITED_WORKFLOWS else
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
        self.assertNotIn('--self-hosted-only', (ROOT/'tools/ci/local-arm').read_text())

    def test_common_ci_has_no_fixed_cargo_or_test_thread_limits(self):
        for path in (ROOT/'.github/workflows').iterdir():
            if path.suffix not in ('.yml', '.yaml') or path.name in INHERITED_WORKFLOWS:
                continue
            document = load(path)
            for name, job in document.get('jobs', {}).items():
                with self.subTest(workflow=path.name, job=name):
                    for field in ('CARGO_BUILD_JOBS', 'RUST_TEST_THREADS'):
                        self.assertNotIn(field, job.get('env', {}))
                    for step in job.get('steps', []):
                        script = step.get('run', '')
                        self.assertNotIn('CARGO_BUILD_JOBS=2', script)
                        self.assertNotIn('--test-threads=1', script)
        for name in ('check_jpeg_sanitizers.py', 'check_locationd_native.py', 'check_paramsd_native.py',
                     'check_ui_torque_miri.py', 'check_xiaoge_native_memory.py', 'check-disk-space'):
            with self.subTest(helper=name):
                text = (ROOT/'rust/tools'/name).read_text()
                self.assertNotIn('-j2', text)
                self.assertNotIn('CARGO_BUILD_JOBS=', text)
                self.assertNotIn('--test-threads=1', text)

    def test_actions_and_workflows_have_real_env_mappings(self):
        paths = [*(ROOT/'.github').rglob('*.yml'), *(ROOT/'.github').rglob('*.yaml'),
                 ROOT/'rust/crates/carrot-man/ci-jobs.yml']
        for path in paths:
            stack = [load(path)]
            while stack:
                node = stack.pop()
                if isinstance(node, dict):
                    if 'env' in node:
                        with self.subTest(path=str(path)):
                            self.assertIsInstance(node['env'], dict)
                    stack.extend(node.values())
                elif isinstance(node, list):
                    stack.extend(node)

    def test_cargo_jobs_always_apply_the_shared_parallel_policy(self):
        action = load(ROOT/'.github/actions/rust-path/action.yml')
        script = action['runs']['steps'][0]['run']
        for required in ('os.sched_getaffinity(0)', 'os.cpu_count()', "os.environ.get(name)",
                         'CARGO_BUILD_JOBS', 'RUST_TEST_THREADS', 'GITHUB_ENV', 'export'):
            self.assertIn(required, script)
        for path in (ROOT/'.github/workflows').iterdir():
            if path.suffix not in ('.yml', '.yaml'):
                continue
            for name, job in load(path).get('jobs', {}).items():
                steps = job.get('steps', [])
                if not any('cargo ' in step.get('run', '') for step in steps):
                    continue
                with self.subTest(workflow=path.name, job=name):
                    first_cargo = next(i for i, step in enumerate(steps) if 'cargo ' in step.get('run', ''))
                    policies = [step for step in steps[:first_cargo] if step.get('uses') == './.github/actions/rust-path']
                    self.assertTrue(policies)
                    # Full unconditional preparation covers every execution mode.
                    # A mode-specific guest may instead prepare CPU policy only;
                    # do not require one particular spelling of its condition.
                    if 'aarch64' in str(job.get('env', {}).get('LOCAL_CI_ARCH', '')):
                        self.assertTrue(any(not step.get('if') or
                                            step.get('with', {}).get('parallelism-only') == 'true' for step in policies))

    def test_inherited_sync_and_publish_workflows_keep_the_exact_pre_port_bytes(self):
        for name in INHERITED_WORKFLOWS:
            relative = '.github/workflows/'+name
            with self.subTest(workflow=name):
                original = subprocess.check_output(['git', 'show',
                    'ec418df39d3b5820f2751978e3898f0a4d858cbc:'+relative], cwd=ROOT)
                self.assertEqual((ROOT/relative).read_bytes(), original)

    def test_workspace_keeps_exact_support_631_feature_and_process_child_setup(self):
        steps = load(ROOT / '.github/workflows/rust.yml')['jobs']['workspace']['steps']
        scripts = [step['run'] for step in steps if any(
            command in step.get('run', '') for command in
            ('cargo clippy --workspace', 'cargo test --workspace'))]
        self.assertEqual(scripts, [updated for _, updated in support_workspace_command_pairs()])

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
                    originals = [normalize(step['run']) for step in before.get('steps', []) if 'run' in step]
                    actual = Counter(digest(restore_parallel_caps(
                        restore_support_workspace_command(relative, name, step['run']), originals))
                        for step in after.get('steps', []) if 'run' in step)
                    self.assertFalse(Counter(expected) - actual, 'original command changed beyond explicit resource-cap removal')
                    before_steps = Counter((digest(normalize(s['run'])), s.get('if'), s.get('continue-on-error'))
                                           for s in before.get('steps', []) if 'run' in s)
                    after_steps = Counter((digest(restore_parallel_caps(
                        restore_support_workspace_command(relative, name, s['run']), originals)), s.get('if'), s.get('continue-on-error'))
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

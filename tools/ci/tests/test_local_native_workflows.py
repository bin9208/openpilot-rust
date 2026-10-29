"""Retain exact required commands while routing this track to the named runner."""
from collections import Counter
from functools import cache
import hashlib
import json
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = 'e445c9d19c3041bc79d8624589e2eb6ba7d061c6'
SUPPORT_BASE = '3fd7fd051b0af237fb8f75a4260e48707e3ae7c8'
MESSAGING_BASE = 'abea63adf5140cb670a1880fa2c350727a0feda9'
LABELS = ['self-hosted', 'Linux', 'X64', 'dev-md-local']
HOSTED_WORKFLOWS = {'fast-checks.yml', 'integration.yml', 'jetlink.yml', 'native-cache-reuse.yml',
    'naver-apk-tools.yml', 'naver-ci.yml', 'tests.yaml', 'user-docs.yaml', 'wiki-settings-check.yaml',
    'carrot-peer-readiness.yml', 'rust.yml'}
OWNED_WORKFLOWS = {'native-apt-scope.yml', 'native-input-publication.yml', 'native-resource-guard.yml',
    'native-source-entry.yml', 'native-source-reuse.yml', 'native-source-transport.yml',
    'runtime-native-environment.yml', 'runtime-source-build.yml'}
NATIVE_ARM_WORKFLOWS = {'agnos-arm-full.yml', 'runtime-native-environment.yml',
    'native-source-entry.yml', 'native-source-transport.yml', 'arm-routing-validation.yml',
    'agnos-arm-abi-diagnostic.yml'}
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


@cache
def support_workspace_updates():
    # #631 legitimately adds modem helper lifecycle coverage to two commands.
    # Match whole immutable scripts, never strip a feature flag generally.
    def scripts(ref):
        document = yaml.load(subprocess.check_output(
            ['git', 'show', ref+':.github/workflows/rust.yml'], cwd=ROOT, text=True),
            Loader=yaml.BaseLoader)
        return [step['run'] for step in document['jobs']['workspace']['steps'] if 'run' in step]
    before, after = scripts(BASE), scripts(SUPPORT_BASE)
    assert len(before) == len(after)
    updates = {new: old for old, new in zip(before, after) if old != new}
    assert len(updates) == 2
    return updates


def ci_adapter(script):
    # The original builder is frozen by the ARM source contract. This adapter
    # calls its original main and retains the complete original argument list.
    for name in ('build_plannerd_acados.py', 'build_plannerd_source.py'):
        script = script.replace('python rust/tools/plannerd_ci_builder.py -- rust/tools/' + name + ' ',
                                'python rust/tools/' + name + ' ')
    script = script.replace('python rust/tools/radarcan_ci_builder.py -- rust/tools/radarcan_prepare_assets.py ',
                            'python rust/tools/radarcan_prepare_assets.py ')
    return support_workspace_updates().get(script, script)



def parallelism_adapter(script):
    # The user explicitly removed these fixed CPU caps. Keep the frozen source
    # hashes intact, then compare every other byte of the original command.
    return script.replace('CARGO_BUILD_JOBS=2 ', '').replace(' --test-threads=1', '')


def arm_migration_adapter(script, job):
    # Source-reviewed ARM migration: preserve target flags and checks while the
    # actual host becomes ARM. QCOM's genuine tests move intact to an ARM job.
    if job == 'arm64':
        return (script.replace('qemu-user-static ', '')
            .replace('--runner /usr/bin/qemu-aarch64-static', '--native'))
    if job == 'workspace':
        if '--bin stage-qcom-compiler -- ' in script:
            return (script.replace('sudo apt-get install -y gcc-aarch64-linux-gnu\n', '')
                .replace('CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc ',
                         'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=gcc ')
                .replace('--bin stage-qcom-compiler -- ', '--bin stage-qcom-compiler -- --native ')
                .replace('/usr/bin/qemu-aarch64-static /usr/aarch64-linux-gnu/lib',
                         '/usr/lib/aarch64-linux-gnu'))
        script = script.replace(' cmake qemu-user-static libc6-arm64-cross libstdc++6-arm64-cross strace',
                                ' cmake strace')
        script = script.replace('tools/tests tools/tests -q',
            'tools/tests tools/tests -q --ignore=tools/tests/test_offline_qcom_compiler.py '
            '--ignore=tools/tests/test_offline_qcom_schedule.py')
    return script


def condition(original):
    expression = original.strip()
    if expression.startswith('${{') and expression.endswith('}}'):
        expression = expression[3:-2].strip()
    return '${{ (' + SAFETY + ')' + (' && (' + expression + ')' if expression else '') + ' }}'


class LocalNativeWorkflowTests(unittest.TestCase):
    def test_generic_arm_build_and_static_ipc_probe_execute_without_qemu(self):
        arm = load(ROOT/'.github/workflows/rust.yml')['jobs']['arm64']
        self.assertEqual(arm['runs-on'], 'ubuntu-24.04-arm')
        commands = '\n'.join(step.get('run', '') for step in arm['steps'])
        self.assertNotIn('qemu-aarch64', commands)
        self.assertNotIn('qemu-user-static', commands)
        probe = next(s['run'] for s in arm['steps'] if
                     s.get('name') == 'Build and inspect the static C3X probe candidate')
        self.assertIn('python tools/check_aarch64.py', probe)
        self.assertIn('--native --report', probe)

    def test_genuine_qcom_tests_move_to_native_arm_without_losing_workspace_gate(self):
        jobs = load(ROOT/'.github/workflows/rust.yml')['jobs']
        arm, workspace = jobs['offline-qcom-arm'], jobs['workspace']
        self.assertEqual(arm['runs-on'], 'ubuntu-24.04-arm')
        self.assertEqual(workspace['needs'], ['offline-qcom-arm'])
        self.assertIn('always()', workspace['if'])
        gate = workspace['steps'][0]
        self.assertEqual(gate['env']['ARM_COMPILER_RESULT'], '${{ needs.offline-qcom-arm.result }}')
        self.assertEqual(gate['run'], 'test "$ARM_COMPILER_RESULT" = success')
        self.assertNotIn('continue-on-error', gate)
        current = next(s['run'] for s in workspace['steps'] if
                       s.get('name') == 'Compare captured graphs and original camera warps with Rust')
        native_tests = next(s['run'] for s in arm['steps'] if
                            s.get('name') == 'Require genuine compilation and offline model scheduling on ARM')
        for filename in ('test_offline_qcom_compiler.py', 'test_offline_qcom_schedule.py'):
            self.assertIn('--ignore=tools/tests/'+filename, current)
            self.assertIn('tools/tests/'+filename, native_tests)
        self.assertNotIn('--ignore', native_tests)
        self.assertNotIn('--deselect', native_tests)
        self.assertTrue(all('continue-on-error' not in s for s in arm['steps']))
        native_stage = next(s['run'] for s in arm['steps'] if
                            s.get('name') == 'Stage the genuine pinned A630 compiler for native execution')
        self.assertIn('--bin stage-qcom-compiler -- --native ', native_stage)
        self.assertNotIn('qemu-aarch64', native_stage)

    def test_merged_support_workspace_keeps_both_complete_lifecycle_commands(self):
        updates = support_workspace_updates()
        job = load(ROOT/'.github/workflows/rust.yml')['jobs']['workspace']
        actual = Counter(step['run'] for step in job['steps'] if 'run' in step)
        self.assertFalse(Counter(updates.keys()) - actual)

    def test_ci_admission_keeps_the_immutable_dbc_generator_bytes(self):
        contract = json.loads((ROOT/'rust/native-dbc-source-contract.json').read_text())
        relative = 'rust/tools/radarcan_prepare_assets.py'
        self.assertIn(relative, contract['roots'])
        original = subprocess.check_output(['git', 'show', contract['original_source_commit']+':'+relative], cwd=ROOT)
        self.assertEqual((ROOT/relative).read_bytes(), original)

    def test_ci_admission_keeps_the_immutable_arm_mpc_builder_bytes(self):
        contract = json.loads((ROOT/'rust/native-mpc-source-contract.json').read_text())
        relative = 'rust/tools/build_plannerd_acados.py'
        self.assertIn(relative, contract['roots'])
        original = subprocess.check_output(['git', 'show', contract['original_source_commit']+':'+relative], cwd=ROOT)
        self.assertEqual((ROOT/relative).read_bytes(), original)

    def test_source_build_uses_real_user_permissions_after_root_only_package_bootstrap(self):
        job = load(ROOT/'.github/workflows/runtime-source-build.yml')['jobs']['offline-cargo-reuse']
        self.assertEqual(job['name'], "Actual offline Cargo reuse (${{ matrix.runner == 'ubuntu-24.04-arm' && 'ARM native' || 'X64' }})")
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
                     'native-resource-guard.yml', 'native-source-reuse.yml',
                     'runtime-native-environment.yml',
                     'native-apt-scope.yml', 'carrot-peer-readiness.yml'):
            with self.subTest(workflow=name):
                policy = load(ROOT / '.github/workflows' / name)['concurrency']
                self.assertEqual(policy['queue'], 'max')
                self.assertEqual(policy['cancel-in-progress'], 'false')
                self.assertTrue(policy['group'].endswith('-parallel-v2'))

    def test_jobs_use_approved_parallel_routes_without_reactivating_inherited_publishers(self):
        for path in sorted((ROOT / '.github/workflows').glob('*')):
            if path.suffix not in ('.yml', '.yaml'):
                continue
            document = load(path)
            if path.name == 'messaging-allocation-431.yml':
                # This independently merged workflow already uses two native
                # hosted runners. Preserve every original command and trigger,
                # including its hosted fork checks, by comparing the whole job.
                original = yaml.load(subprocess.check_output(
                    ['git', 'show', MESSAGING_BASE+':.github/workflows/'+path.name],
                    cwd=ROOT, text=True), Loader=yaml.BaseLoader)
                self.assertEqual(document, original)
                self.assertEqual(set(document['jobs']), {'comparison'})
                comparison = document['jobs']['comparison']
                self.assertEqual(comparison['runs-on'], '${{ matrix.runner }}')
                self.assertEqual(comparison['strategy']['matrix']['runner'],
                                 ['ubuntu-24.04', 'ubuntu-24.04-arm'])
                continue
            for name, job in document.get('jobs', {}).items():
                if 'runs-on' in job:
                    with self.subTest(path=path.name, job=name):
                        if path.name in NATIVE_ARM_WORKFLOWS:
                            expected = 'ubuntu-24.04-arm'
                        elif path.name == 'native-source-reuse.yml':
                            expected = '${{ matrix.arch == \'aarch64\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}'
                        elif path.name == 'runtime-source-build.yml' and name == 'offline-cargo-reuse':
                            expected = '${{ matrix.runner == \'ubuntu-24.04-arm\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}'
                        elif path.name in OWNED_WORKFLOWS:
                            expected = ['self-hosted', 'Linux', 'X64', 'openpilot-rust']
                        elif path.name not in HOSTED_WORKFLOWS:
                            expected = LABELS  # Inherited sync/publish routing stays unchanged.
                        elif path.name == 'tests.yaml' and name == 'build_release':
                            expected = ['self-hosted', 'Linux', 'X64', 'openpilot-rust']
                        elif path.name == 'rust.yml' and name in {
                                'support-runtime', 'telemetry-runtime', 'web-upload-timeouts', 'joystickd-runtime',
                                'encoder-runtime', 'camera-runtime', 'panda-runtime', 'selfdrive-runtime',
                                'radar-runtime', 'navd-runtime', 'carrot-navi-runtime', 'startup-runtime',
                                'startup-services', 'sensor-audio', 'gnss-runtime', 'controls-runtime',
                                'estimation-runtime', 'ui-connectivity', 'platform-runtime', 'hardware-runtime',
                                'logger-runtime', 'athena-runtime'}:
                            expected = 'ubuntu-24.04'
                        elif ('runner' in job.get('strategy', {}).get('matrix', {})
                              or any('runner' in row for row in job.get('strategy', {}).get('matrix', {}).get('include', []))):
                            expected = '${{ matrix.runner == \'ubuntu-24.04-arm\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}'
                        elif path.name == 'rust.yml' and name != 'fast':
                            expected = ('ubuntu-24.04-arm' if job.get('env', {}).get('LOCAL_CI_ARCH') == 'aarch64'
                                        else ['self-hosted', 'Linux', 'X64', 'openpilot-rust'])
                        elif path.name == 'jetlink.yml' and name == 'model':
                            expected = ['self-hosted', 'Linux', 'X64', 'openpilot-rust']
                        elif job.get('env', {}).get('LOCAL_CI_ARCH') == 'aarch64':
                            expected = 'ubuntu-24.04-arm'
                        else:
                            expected = 'ubuntu-24.04'
                        self.assertEqual(job['runs-on'], expected)
                        self.assertIn(SAFETY, job['if'])
        template = load(ROOT / 'rust/crates/carrot-man/ci-jobs.yml')['carrot-man-runtime']
        self.assertEqual(template['runs-on'], '${{ matrix.runner == \'ubuntu-24.04-arm\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}')
        owner = (ROOT / '.github/actions/job-temp/action.yml').read_text()
        self.assertIn('runner_policy.py', owner)
        policy = (ROOT/'tools/ci/runner_policy.py').read_text()
        for admission in ('bin-actions', 'bin-actions2', 'bin-actions3', 'github-hosted'):
            self.assertIn(admission, policy)

    def test_frozen_jobs_dependencies_matrix_and_each_original_command_survive(self):
        snapshot = json.loads((ROOT / 'tools/ci/fixtures/local-native-workflows-605.json').read_text())
        self.assertEqual(snapshot['schema'], 1)
        self.assertEqual(snapshot['source_commit'], BASE)
        for relative, frozen in snapshot['workflows'].items():
            original = yaml.load(subprocess.check_output(['git', 'show', BASE + ':' + relative],
                                                        cwd=ROOT, text=True), Loader=yaml.BaseLoader)
            current = load(ROOT / relative)
            additions = {'offline-qcom-arm'} if relative == '.github/workflows/rust.yml' else set()
            self.assertEqual(set(current['jobs']), set(original['jobs']) | additions)
            for name, before in original['jobs'].items():
                with self.subTest(path=relative, job=name):
                    after = current['jobs'][name]
                    expected_needs = ['offline-qcom-arm'] if name == 'workspace' else before.get('needs')
                    self.assertEqual(after.get('needs'), expected_needs)
                    self.assertEqual(after.get('strategy', {}).get('matrix'), before.get('strategy', {}).get('matrix'))
                    expected = [digest(normalize(step['run'])) for step in before.get('steps', []) if 'run' in step]
                    self.assertEqual(frozen['jobs'][name]['scripts'], expected)
                    parallel_expected = [digest(parallelism_adapter(arm_migration_adapter(normalize(step['run']), name)))
                                         for step in before.get('steps', []) if 'run' in step]
                    compared_steps = after.get('steps', [])
                    if name == 'workspace':
                        compared_steps = [*compared_steps, *current['jobs']['offline-qcom-arm']['steps']]
                    actual = Counter(digest(parallelism_adapter(ci_adapter(step['run'])))
                                     for step in compared_steps if 'run' in step)
                    self.assertFalse(Counter(parallel_expected) - actual, 'original command changed or removed')
                    before_steps = Counter((digest(parallelism_adapter(arm_migration_adapter(normalize(s['run']), name))), s.get('if'), s.get('continue-on-error'))
                                           for s in before.get('steps', []) if 'run' in s)
                    after_steps = Counter((digest(parallelism_adapter(ci_adapter(s['run']))), s.get('if'), s.get('continue-on-error'))
                                          for s in compared_steps if 'run' in s)
                    self.assertFalse(before_steps - after_steps, 'original command gained an escape/skip')
                    if 'runs-on' in before:
                        expected_if = condition(before.get('if', ''))
                        if name == 'workspace':
                            expected_if = expected_if.replace('${{ ', '${{ always() && ', 1)
                        self.assertEqual(after['if'], expected_if)
                    for event, config in original.get('on', {}).items():
                        if relative == '.github/workflows/fast-checks.yml' and event == 'push':
                            self.assertNotIn('push', current['on'])
                            continue  # User explicitly retired parallel push Fast checks.
                        self.assertIn(event, current['on'])
                        if isinstance(config, dict) and 'branches' in config:
                            self.assertTrue(set(config['branches']).issubset(current['on'][event]['branches']))

    def test_arm_variants_preserve_native_or_guest_execution_evidence(self):
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

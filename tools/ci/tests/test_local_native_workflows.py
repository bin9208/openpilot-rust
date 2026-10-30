"""Retain exact required commands while routing this track to the named runner."""
from collections import Counter
from functools import cache
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import unittest
from unittest import mock

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = 'e445c9d19c3041bc79d8624589e2eb6ba7d061c6'
SUPPORT_BASE = '3fd7fd051b0af237fb8f75a4260e48707e3ae7c8'
MESSAGING_BASE = 'abea63adf5140cb670a1880fa2c350727a0feda9'
NATIVE_ARM_BASE = '27547f08c6daf7deac99e35b0f6e1d0c8ffb0298'
LABELS = ['self-hosted', 'Linux', 'X64', 'dev-md-local']
HOSTED_WORKFLOWS = {'fast-checks.yml', 'integration.yml', 'jetlink.yml', 'native-cache-reuse.yml',
    'naver-apk-tools.yml', 'naver-ci.yml', 'tests.yaml', 'user-docs.yaml', 'wiki-settings-check.yaml',
    'carrot-peer-readiness.yml', 'rust.yml'}
OWNED_WORKFLOWS = {'native-apt-scope.yml', 'native-input-publication.yml', 'native-resource-guard.yml',
    'native-source-entry.yml', 'native-source-reuse.yml', 'native-source-transport.yml',
    'runtime-native-environment.yml', 'runtime-source-build.yml'}
NATIVE_ARM_ROUTES = {
    ('native-source-entry.yml', 'entry'): 'ubuntu-24.04-arm',
    ('native-source-transport.yml', 'admission'): 'ubuntu-24.04-arm',
    ('runtime-native-environment.yml', 'compiler'): 'ubuntu-24.04-arm',
    ('native-source-reuse.yml', 'real-runtime'): '${{ matrix.arch == \'aarch64\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}',
}
SAFETY = "github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository"
HOSTED_CLEANUP = 'sudo rm -rf /usr/local/lib/android /opt/ghc /usr/share/dotnet'
NATIVE_ARM_SCRIPTS = {
    'Reserve capacity for generic ARM build dependencies': '46080f5ced2551f300410fa298ee641be5ceeecbd575fbd435eff28998f3369d',
    'Stage native GNU aarch64 startup renderer and portable loader headers': 'b58cbf4a49bf163229feaffaa33307f20e360a29143e71503235b25a0512fd54',
    'Build and inspect the static C3X probe candidate': 'e8ea9b91195f5dc37280f9c9eeb33ee198a241d330559dc378c038b5093ec94d',
}
# These workflows postdate BASE. Freeze their exact reviewed host preparation
# against H27547 separately; the inherited fixture and its comparisons stay intact.
NATIVE_OWNED_SCRIPTS = {
    'native-source-entry.yml': {
        'Require the GitHub-hosted Linux ARM64 source entry runner': '88805623db62ac4e117f3ab45bc8a70f38e4930cbc044869dc6faa146e2a6cb5',
        'Admit only this operation and install its host transport tools': 'b338f3965e05646d896173c20f7fe8a63e49d56b92d0a1b56ffa928c388aeb99',
    },
    'native-source-transport.yml': {
        'Require the GitHub-hosted Linux ARM64 factory admission runner': '88805623db62ac4e117f3ab45bc8a70f38e4930cbc044869dc6faa146e2a6cb5',
        'Admit the image and tool operation before allocating its files': '5db2f4e771e459b1e3c7ce204da2d67b1532957b4ade91b46a975e38d22171d1',
    },
    'runtime-native-environment.yml': {
        'Admit this host tool and extract operation': 'a8f29b03f073d10d7ccb13847f0129c3f21d0aa880a4aeacb4c8ac7aa399ba15',
        'Prepare only this bootstrap job kernel scope': 'a064e70610c4026fabcb73c73197f55f4c09b1de564140e39a97423c080076a6',
        'Validate the current package growth policy before deep ARM preparation': '31cc69fd7f9a618858b1b68ce623790d65fa83638373fc053684f8a6128f2d8b',
        'Build the pinned original planner ABI before deep ARM preparation': '0f744307858558c68e223e58936f3b8feacd6d56c713ed62b639547439fb0130',
        'Stop and remove only the bootstrap job kernel scope': '790f8d0e9ca2d60b658c7871776bee48508b76e7afa96742a53bf9e7f9cb0d94',
    },
    'native-source-reuse.yml': {
        'Require the GitHub-hosted Linux ARM64 reuse runner': '01c0d20ecdfb201805299a4ba368b31a6685e8a79f647fd0c12fa66fcc9c0d1f',
    },
}


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


@cache
def native_arm_updates():
    # Only three complete, source-reviewed scripts of this exact job may
    # replace their frozen X64 cross-host setup and explicit QEMU invocation.
    before = yaml.load(subprocess.check_output(
        ['git', 'show', BASE+':.github/workflows/rust.yml'], cwd=ROOT, text=True),
        Loader=yaml.BaseLoader)['jobs']['arm64']
    after = load(ROOT/'.github/workflows/rust.yml')['jobs']['arm64']
    updates = {}
    for name, expected in NATIVE_ARM_SCRIPTS.items():
        old = next(step for step in before['steps'] if step.get('name') == name)
        new = next(step for step in after['steps'] if step.get('name') == name)
        assert digest(new['run']) == expected, 'unreviewed native ARM command: '+name
        assert {k: v for k, v in old.items() if k != 'run'} == {k: v for k, v in new.items() if k != 'run'}
        updates[new['run']] = normalize(old['run'])
    return updates


def ci_adapter(script, *, native_arm=False):
    # The original builder is frozen by the ARM source contract. This adapter
    # calls its original main and retains the complete original argument list.
    for name in ('build_plannerd_acados.py', 'build_plannerd_source.py'):
        script = script.replace('python rust/tools/plannerd_ci_builder.py -- rust/tools/' + name + ' ',
                                'python rust/tools/' + name + ' ')
    script = script.replace('python rust/tools/radarcan_ci_builder.py -- rust/tools/radarcan_prepare_assets.py ',
                            'python rust/tools/radarcan_prepare_assets.py ')
    if native_arm:
        script = native_arm_updates().get(script, script)
    return support_workspace_updates().get(script, script)



def parallelism_adapter(script):
    # The user explicitly removed these fixed CPU caps. Keep the frozen source
    # hashes intact, then compare every other byte of the original command.
    return script.replace('CARGO_BUILD_JOBS=2 ', '').replace(' --test-threads=1', '')


def condition(original):
    expression = original.strip()
    if expression.startswith('${{') and expression.endswith('}}'):
        expression = expression[3:-2].strip()
    return '${{ (' + SAFETY + ')' + (' && (' + expression + ')' if expression else '') + ' }}'


class LocalNativeWorkflowTests(unittest.TestCase):
    def test_owned_native_arm_jobs_change_only_reviewed_host_preparation(self):
        renames = {
            'Require the GitHub-hosted Linux ARM64 source entry runner': 'Require the registered self-hosted source entry runner',
            'Require the GitHub-hosted Linux ARM64 factory admission runner': 'Require the registered self-hosted factory admission runner',
        }
        for (filename, name), route in NATIVE_ARM_ROUTES.items():
            with self.subTest(workflow=filename, job=name):
                relative = '.github/workflows/'+filename
                before = yaml.load(subprocess.check_output(
                    ['git', 'show', NATIVE_ARM_BASE+':'+relative], cwd=ROOT, text=True),
                    Loader=yaml.BaseLoader)
                after = load(ROOT/relative)
                if filename == 'native-source-transport.yml':
                    branches = before['on']['push']['branches']
                    self.assertEqual(after['on']['push']['branches'], ['codex/feat-599-*', *branches])
                    after['on']['push']['branches'] = branches
                old, new = before['jobs'][name], after['jobs'][name]
                self.assertEqual(new['runs-on'], route)
                new['runs-on'] = old['runs-on']
                if filename in ('native-source-entry.yml', 'native-source-transport.yml'):
                    self.assertEqual(new['env']['LOCAL_CI_ARCH'], 'aarch64')
                    self.assertEqual(new['env']['SCOPE_CONTEXT'], old['env']['SCOPE_CONTEXT'].replace(
                        'RUNNER_NAME,', 'RUNNER_NAME,RUNNER_OS,RUNNER_ARCH,'))
                    new['env']['LOCAL_CI_ARCH'] = old['env']['LOCAL_CI_ARCH']
                    new['env']['SCOPE_CONTEXT'] = old['env']['SCOPE_CONTEXT']
                elif filename == 'runtime-native-environment.yml':
                    self.assertEqual(new['env'].pop('LOCAL_CI_ARCH'), 'aarch64')
                for step_name, expected in NATIVE_OWNED_SCRIPTS[filename].items():
                    matches = [step for step in new['steps'] if step.get('name') == step_name]
                    self.assertEqual(len(matches), 1)
                    step = matches[0]
                    self.assertEqual(digest(step['run']), expected, 'unreviewed host command: '+step_name)
                    if filename == 'native-source-reuse.yml':
                        self.assertEqual(new['steps'].index(step), 1)
                        self.assertEqual(step, dict(name=step_name, shell='bash',
                            **{'if': "matrix.arch == 'aarch64'", 'run': step['run']}))
                        new['steps'].remove(step)
                    else:
                        old_name = renames.get(step_name, step_name)
                        original = next(s for s in old['steps'] if s.get('name') == old_name)
                        step['name'], step['run'] = old_name, original['run']
                        self.assertEqual(step, original)
                if filename == 'runtime-native-environment.yml':
                    scope_name = 'Prepare only this bootstrap job kernel scope'
                    self.assertEqual([s.get('name') for s in new['steps'][3:6]], [
                        'Admit this host tool and extract operation', scope_name,
                        'Validate the current package growth policy before deep ARM preparation'])
                    scope = new['steps'].pop(4)
                    original_index = next(i for i, s in enumerate(old['steps']) if s.get('name') == scope_name)
                    new['steps'].insert(original_index, scope)
                self.assertEqual(after, before, 'unreviewed workflow, command, pin, gate or deadline change')

    def test_native_arm_probe_preserves_original_acceptance_and_deadlines(self):
        path = 'rust/tools/check_aarch64.py'
        before = ast.parse(subprocess.check_output(['git', 'show', BASE+':'+path], cwd=ROOT, text=True))
        after = ast.parse((ROOT/path).read_text())
        def contract(tree):
            return Counter(ast.dump(node, include_attributes=False) for node in ast.walk(tree)
                           if isinstance(node, ast.Assert)
                           or isinstance(node, ast.keyword) and node.arg == 'timeout'
                           or isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'deadline' for t in node.targets))
        self.assertFalse(contract(before) - contract(after), 'original ARM probe acceptance/deadline changed')

    def test_direct_arm_probe_rejects_non_arm_host_before_any_execution(self):
        tree = ast.parse((ROOT/'rust/tools/check_aarch64.py').read_text())
        check = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'check')
        platform = mock.Mock()
        platform.machine.return_value = 'x86_64'
        namespace = {'Path': Path, 'platform': platform}
        exec(compile(ast.Module(body=[check], type_ignores=[]), '<native-arm-probe>', 'exec'), namespace)
        with self.assertRaisesRegex(RuntimeError, 'native aarch64 host'):
            namespace['check'](Path('/unused/binary'), None, Path('/unused/report'))

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
                if name == 'rust.yml':
                    self.assertEqual(policy['group'], 'rust-${{ github.event_name }}-${{ github.ref }}-native-arm-v1')
                else:
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
                        if path.name == 'runtime-source-build.yml' and name == 'offline-cargo-reuse':
                            expected = '${{ matrix.runner == \'ubuntu-24.04-arm\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}'
                        elif (path.name, name) in NATIVE_ARM_ROUTES:
                            expected = NATIVE_ARM_ROUTES[path.name, name]
                        elif path.name in OWNED_WORKFLOWS:
                            expected = ['self-hosted', 'Linux', 'X64', 'openpilot-rust']
                        elif path.name not in HOSTED_WORKFLOWS:
                            expected = LABELS  # Inherited sync/publish routing stays unchanged.
                        elif path.name == 'tests.yaml' and name == 'build_release':
                            expected = ['self-hosted', 'Linux', 'X64', 'openpilot-rust']
                        elif path.name == 'rust.yml' and name == 'arm64':
                            expected = 'ubuntu-24.04-arm'
                        elif path.name == 'rust.yml' and name in {
                                'support-runtime', 'telemetry-runtime', 'web-upload-timeouts', 'joystickd-runtime',
                                'encoder-runtime', 'camera-runtime', 'panda-runtime', 'selfdrive-runtime',
                                'radar-runtime', 'navd-runtime', 'carrot-navi-runtime', 'startup-runtime',
                                'startup-services', 'sensor-audio', 'gnss-runtime', 'controls-runtime',
                                'estimation-runtime', 'ui-connectivity', 'platform-runtime', 'hardware-runtime',
                                'logger-runtime', 'athena-runtime', 'planner-memory', 'radar-memory'}:
                            expected = 'ubuntu-24.04'
                        elif path.name == 'rust.yml' and name in {
                                'radard-runtime', 'carrot-man-runtime', 'webrtc-runtime', 'webcam-runtime'}:
                            expected = '${{ matrix.runner }}'
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
            self.assertEqual(set(current['jobs']), set(original['jobs']))
            for name, before in original['jobs'].items():
                with self.subTest(path=relative, job=name):
                    after = current['jobs'][name]
                    native_arm = relative == '.github/workflows/rust.yml' and name == 'arm64'
                    self.assertEqual(after.get('needs'), before.get('needs'))
                    self.assertEqual(after.get('strategy', {}).get('matrix'), before.get('strategy', {}).get('matrix'))
                    expected = [digest(normalize(step['run'])) for step in before.get('steps', []) if 'run' in step]
                    self.assertEqual(frozen['jobs'][name]['scripts'], expected)
                    parallel_expected = [digest(parallelism_adapter(normalize(step['run'])))
                                         for step in before.get('steps', []) if 'run' in step]
                    actual = Counter(digest(parallelism_adapter(ci_adapter(step['run'], native_arm=native_arm)))
                                     for step in after.get('steps', []) if 'run' in step)
                    self.assertFalse(Counter(parallel_expected) - actual, 'original command changed or removed')
                    before_steps = Counter((digest(parallelism_adapter(normalize(s['run']))), s.get('if'), s.get('continue-on-error'))
                                           for s in before.get('steps', []) if 'run' in s)
                    after_steps = Counter((digest(parallelism_adapter(ci_adapter(s['run'], native_arm=native_arm))), s.get('if'), s.get('continue-on-error'))
                                          for s in after.get('steps', []) if 'run' in s)
                    self.assertFalse(before_steps - after_steps, 'original command gained an escape/skip')
                    if 'runs-on' in before:
                        self.assertEqual(after['if'], condition(before.get('if', '')))
                    for event, config in original.get('on', {}).items():
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

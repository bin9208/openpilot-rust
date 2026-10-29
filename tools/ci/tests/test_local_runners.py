"""Keep all required checks across hosted Linux and the two registered runners."""
import importlib.util
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location('tested_runner_policy', ROOT/'tools/ci/runner_policy.py')
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)
LOCAL = ['self-hosted', 'Linux', 'X64', 'dev-md-local']
ARM_JOBS = ('planner-runtime', 'radar-arm', 'radard-runtime', 'carrot-navi-arm',
            'xiaoge-runtime', 'carrot-man-runtime', 'webrtc-runtime', 'webcam-runtime')


def workflow(path):
    return yaml.load(path.read_text(), Loader=yaml.BaseLoader)


class LocalRunnerPolicyTests(unittest.TestCase):
    def test_only_approved_routes_are_used(self):
        for path in sorted((ROOT/'.github/workflows').glob('*')):
            for name, job in workflow(path).get('jobs', {}).items():
                if 'uses' in job:
                    continue
                route = job.get('runs-on')
                with self.subTest(workflow=path.name, job=name):
                    if route == '${{ matrix.runner }}':
                        matrix = job['strategy']['matrix']
                        runners = matrix.get('runner') or [row['runner'] for row in matrix['include']]
                        self.assertEqual(runners, ['ubuntu-24.04', 'ubuntu-24.04-arm'])
                    else:
                        self.assertTrue(route in ('ubuntu-24.04', 'ubuntu-24.04-arm') or
                                        route in (LOCAL, ['self-hosted', 'Linux', 'X64', 'support-630'],
                                                  ['self-hosted', 'Linux', 'X64']))

    def test_three_compute_pools_are_routed_explicitly(self):
        jobs = workflow(ROOT/'.github/workflows/ci.yml')['jobs']
        self.assertEqual(jobs['workspace']['runs-on'], 'ubuntu-24.04')
        self.assertEqual(jobs['build_release']['runs-on'], ['self-hosted', 'Linux', 'X64', 'support-630'])
        self.assertEqual(jobs['arm-build']['runs-on'], LOCAL)
        self.assertEqual(jobs['runtime-delivery']['runs-on'], LOCAL)

    def test_authorized_runtime_delivery_can_use_either_registered_runner(self):
        jobs = workflow(ROOT/'.github/workflows/dev-md-runtime.yml')['jobs']
        self.assertEqual(set(jobs), {'verify', 'package', 'publish'})
        for name, job in jobs.items():
            self.assertEqual(job['runs-on'], ['self-hosted', 'Linux', 'X64'], name)
        self.assertEqual(jobs['package']['needs'], 'verify')
        self.assertEqual(jobs['publish']['needs'], ['verify', 'package'])

    def test_saved_template_remains_a_supported_fallback(self):
        job = workflow(ROOT/'rust/crates/carrot-man/ci-jobs.yml')['carrot-man-runtime']
        self.assertEqual(job['runs-on'], LOCAL)
        self.assertEqual(job['defaults']['run']['shell'], 'openpilot-local-shell {0}')
        self.assertIn('LOCAL_CI_ARCH', job['env'])

    def test_arm_commands_and_execution_evidence_remain_required(self):
        jobs = workflow(ROOT/'.github/workflows/ci.yml')['jobs']
        for name in ARM_JOBS:
            job = jobs[name]
            with self.subTest(job=name):
                self.assertIn(job['runs-on'], ('ubuntu-24.04-arm', '${{ matrix.runner }}'))
                self.assertEqual(job['defaults']['run']['shell'], 'openpilot-local-shell {0}')
                transport = [s for s in job['steps'] if s.get('uses') == './.github/actions/local-arm']
                self.assertEqual(len(transport), 2)
                self.assertNotIn('if', transport[0])
                self.assertEqual(transport[-1].get('if'), 'always()')
                condition = "runner.arch == 'ARM64' && env.LOCAL_CI_ARCH == 'aarch64'"
                prepare_index = job['steps'].index(transport[0])
                pinned = [s for s in job['steps'][:prepare_index]
                          if s.get('uses') == 'actions/setup-python@v6' and s.get('if') == condition]
                rust = [s for s in job['steps'][:prepare_index]
                        if s.get('uses') == './.github/actions/rust-path' and s.get('if') == condition]
                self.assertEqual(len(pinned), 1)
                self.assertEqual(pinned[0]['with']['python-version'], '3.12.14')
                self.assertEqual(len(rust), 1)
                for step in job['steps'][prepare_index+1:]:
                    if step.get('uses', '').startswith('actions/setup-python@'):
                        self.assertEqual(step.get('if'), "env.LOCAL_CI_ARCH == 'x86_64'")
                for step in job['steps']:
                    if step.get('uses', '').startswith('actions/upload-artifact@'):
                        paths = step['with']['path']
                        self.assertIn('local-arm/execution.json', paths)
                        self.assertNotIn('rust/target', paths)
                        if name == 'radard-runtime':
                            self.assertIn('${{ env.CI_JOB_TEMP }}/radard-native/', paths)
                            commands = '\n'.join(item.get('run', '') for item in job['steps'])
                            self.assertIn('rust/target/debug/openpilot-radard', commands)
                            self.assertIn('cp "$radard_binary" "$RUNNER_TEMP/radard-native/bin/"', commands)
                        else:
                            self.assertIn('LOCAL_CI_TARGET_DIR', paths)

    def test_native_fast_path_and_pinned_qemu_fallback_both_remain(self):
        text = (ROOT/'tools/ci/local-arm').read_text()
        for required in ('runner_policy.py', '$host_machine == "$LOCAL_CI_ARCH"', 'execution_mode',
                         'native-apt-shell', 'qemu-user', 'physical_device_execution',
                         'qemu-aarch64-static', 'proot', 'sha256sum', 'CI_JOB_TEMP'):
            self.assertIn(required, text)
        self.assertLess(text.index('LOCAL_CI_EXECUTION_MODE=native'), text.index('download \'https://'))

    def test_environment_policy_accepts_two_registered_and_native_hosted_architectures(self):
        for name in ('bin-actions', 'bin-actions2'):
            self.assertTrue(POLICY.environment_allowed(dict(GITHUB_ACTIONS='true',
                            RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME=name)))
        for arch in ('X64', 'ARM64'):
            env = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted',
                       RUNNER_OS='Linux', RUNNER_ARCH=arch)
            self.assertTrue(POLICY.environment_allowed(env))
            self.assertFalse(POLICY.environment_allowed(env, self_hosted_only=True))
            for key, value in (('GITHUB_ACTIONS', 'false'), ('RUNNER_OS', 'Windows'), ('RUNNER_ARCH', 'S390X')):
                self.assertFalse(POLICY.environment_allowed(dict(env, **{key: value})))
        self.assertFalse(POLICY.environment_allowed(dict(GITHUB_ACTIONS='true',
                         RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='unregistered')))

    def test_api_job_policy_preserves_registered_names_and_hosted_labels(self):
        for label in ('ubuntu-24.04', 'ubuntu-24.04-arm'):
            self.assertTrue(POLICY.job_is_allowed(dict(runner_name='GitHub Actions 1', labels=[label])))
            self.assertFalse(POLICY.job_is_allowed(dict(runner_name='GitHub Actions 1', labels=[label]), self_hosted_only=True))
        for name in ('bin-actions', 'bin-actions2'):
            self.assertTrue(POLICY.job_is_allowed(dict(runner_name=name, labels=LOCAL)))
        self.assertFalse(POLICY.job_is_allowed(dict(runner_name='unregistered', labels=LOCAL)))
        self.assertFalse(POLICY.job_is_allowed(dict(runner_name='GitHub Actions 1', labels=['ubuntu-latest'])))
        for malformed in (None, [], 'job', {'labels': 'ubuntu-24.04'},
                          {'labels': [None]}, {'labels': ['ubuntu-24.04']},
                          {'labels': ['ubuntu-24.04'], 'runner_name': ' '}):
            self.assertFalse(POLICY.job_is_allowed(malformed))
        for malformed in (None, [], 'environment'):
            self.assertFalse(POLICY.environment_allowed(malformed))

    def test_untrusted_fork_source_cannot_reach_registered_jobs(self):
        jobs = workflow(ROOT/'.github/workflows/ci.yml')['jobs']
        for name in ('route', 'integration-gate', 'fast', 'arm64'):
            self.assertIn('github.event.pull_request.head.repo.full_name', jobs[name].get('if', ''))
            self.assertIn('github.repository', jobs[name].get('if', ''))

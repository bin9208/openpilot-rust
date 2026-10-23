"""Require every workflow to use the owner-selected local runner without losing ARM coverage."""
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]
LOCAL = ['self-hosted', 'Linux', 'X64', 'dev-md-local']
ARM_JOBS = ('planner-runtime', 'radar-arm', 'radard-runtime', 'carrot-navi-arm',
            'xiaoge-runtime', 'carrot-man-runtime', 'webrtc-runtime', 'webcam-runtime')


def workflow(path):
    return yaml.load(path.read_text(), Loader=yaml.BaseLoader)


class LocalRunnerPolicyTests(unittest.TestCase):
    def test_all_jobs_in_all_workflows_use_the_local_runner(self):
        for path in sorted((ROOT / '.github/workflows').glob('*')):
            for name, job in workflow(path).get('jobs', {}).items():
                with self.subTest(workflow=path.name, job=name):
                    if 'uses' not in job:
                        self.assertEqual(job.get('runs-on'), LOCAL)

    def test_saved_job_template_uses_the_same_local_policy(self):
        job = workflow(ROOT / 'rust/crates/carrot-man/ci-jobs.yml')['carrot-man-runtime']
        self.assertEqual(job['runs-on'], LOCAL)
        self.assertEqual(job['defaults']['run']['shell'], 'openpilot-local-shell {0}')
        self.assertIn('LOCAL_CI_ARCH', job['env'])

    def test_arm_execution_is_preserved_in_a_distinct_emulated_environment(self):
        jobs = workflow(ROOT / '.github/workflows/ci.yml')['jobs']
        for name in ARM_JOBS:
            with self.subTest(job=name):
                job = jobs[name]
                self.assertIn('LOCAL_CI_ARCH', job.get('env', {}))
                self.assertEqual(job.get('defaults', {}).get('run', {}).get('shell'),
                                 'openpilot-local-shell {0}')
                transport = [step for step in job['steps']
                             if step.get('uses') == './.github/actions/local-arm']
                self.assertEqual(len(transport), 2)
                self.assertNotIn('if', transport[0])
                self.assertEqual(transport[-1].get('if'), 'always()')
                self.assertEqual(transport[-1].get('with', {}).get('cleanup'), 'true')
                for step in job['steps']:
                    if step.get('uses', '').startswith('actions/setup-python@'):
                        self.assertIn("LOCAL_CI_ARCH == 'x86_64'", step.get('if', ''))

    def test_arm_uploads_match_the_guest_target_and_include_execution_evidence(self):
        jobs = workflow(ROOT / '.github/workflows/ci.yml')['jobs']
        for name in ARM_JOBS:
            for step in jobs[name]['steps']:
                if step.get('uses', '').startswith('actions/upload-artifact@'):
                    paths = step['with']['path']
                    self.assertNotIn('rust/target', paths, name)
                    if name == 'radard-runtime':
                        self.assertIn('${{ env.CI_JOB_TEMP }}/radard-native/', paths)
                        commands = '\n'.join(item.get('run', '') for item in jobs[name]['steps'])
                        self.assertIn('rust/target/debug/openpilot-radard', commands)
                        self.assertIn('cp "$radard_binary" "$RUNNER_TEMP/radard-native/bin/"', commands)
                    else:
                        self.assertIn('LOCAL_CI_TARGET_DIR', paths, name)
                    self.assertIn('local-arm/execution.json', paths, name)

    def test_arm_logical_targets_do_not_claim_a_hosted_or_native_arm_runner(self):
        text = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertNotIn('ubuntu-24.04-arm', text)
        self.assertIn('local-arm64-qemu', text)
        self.assertIn('local-x64', text)

    def test_untrusted_fork_source_is_not_scheduled_on_the_persistent_runner(self):
        jobs = workflow(ROOT / '.github/workflows/ci.yml')['jobs']
        for name in ('route', 'integration-gate', 'fast', 'arm64'):
            condition = jobs[name].get('if', '')
            self.assertIn('github.event.pull_request.head.repo.full_name', condition, name)
            self.assertIn('github.repository', condition, name)
        wiki = workflow(ROOT / '.github/workflows/wiki-settings-check.yaml')['jobs']['check']
        self.assertIn('github.event.pull_request.head.repo.full_name', wiki.get('if', ''))

    def test_arm_transport_checks_real_runner_and_records_emulation(self):
        path = ROOT / 'tools/ci/local-arm'
        self.assertTrue(path.is_file(), 'ARM execution transport is required')
        text = path.read_text()
        for required in ('RUNNER_ENVIRONMENT', 'self-hosted', 'qemu-aarch64-static',
                         'proot', 'execution_mode', 'qemu-user', 'host_architecture',
                         'guest_architecture', 'sha256sum', 'CI_JOB_TEMP'):
            self.assertIn(required, text)

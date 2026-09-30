"""Prevent inherited publishers from acting in the independent Rust repository."""
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]

class RustIsolationTests(unittest.TestCase):
    def test_hardware_runtime_requires_source_and_native_boundaries(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['hardware-runtime']
        self.assertNotIn('if', job)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_bootlog.py', 'check_bootlog_snapshot.py', 'check_logger_identifier.py',
                         'check_hardware_info.py', 'check_hardware_control.py', 'check_amplifier.py',
                         'check_amplifier_linux.py', 'bootlog_raw_file_probe.rs'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)

    def test_startup_prerequisites_run_actual_children_and_collectors(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['startup-runtime']
        self.assertNotIn('if', job)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_process_supervision.py', 'check_managed_entry.py', 'check_sdk_receiver.py',
                         'check_registration.py', 'check_registration_clock.py', 'check_registration_utf7.py',
                         'check_registration_vendor.py', 'check_manager_catalog.py', 'check_checkout_status.py', '--timeouts',
                         '--launcher rust/target/debug/openpilot-process-child',
                         '--fixture rust/target/debug/examples/process_fixture',
                         'build_msgq_python.py', 'build_params_python.py'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)
        artifacts = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(artifacts), 1)
        self.assertEqual(artifacts[0]['if'], 'always()')

    def test_interrupted_send_boundary_is_a_required_support_check(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        steps = data['jobs']['support-runtime']['steps']
        matches = [step for step in steps if step.get('name') == 'Compare interrupted diagnostic sends and actual upload completion']
        self.assertEqual(len(matches), 1)
        step = matches[0]
        self.assertNotIn('if', step)
        self.assertNotIn('continue-on-error', step)
        for required in ('zmq_send_boundary.c', '--wrap=zmq_msg_send', 'check_interrupted_send.py', 'check_uploader_interrupted.py'):
            self.assertIn(required, step['run'])

    def test_root_directory_builds_select_the_pinned_rust_toolchain(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        channel = tomllib.loads((ROOT / 'rust/rust-toolchain.toml').read_text())['toolchain']['channel']
        self.assertEqual(data.get('env', {}).get('RUSTUP_TOOLCHAIN'), channel)

    def test_support_binding_path_is_configured_on_the_runner(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        for name, job in data['jobs'].items():
            for value in job.get('env', {}).values():
                with self.subTest(job=name):
                    self.assertNotRegex(value, r'\$\{\{\s*runner[.\[]')
        support = data['jobs']['support-runtime']
        setup = next(step for step in support['steps'] if step.get('name') == 'Configure original support IPC imports')
        with tempfile.TemporaryDirectory(prefix='support env ') as temporary:
            output = Path(temporary) / 'environment'
            environment = {'RUNNER_TEMP': temporary, 'PYTHONPATH': '/fixture/repository:/fixture/tools', 'GITHUB_ENV': str(output)}
            subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', setup['run']], env=environment, check=True)
            self.assertEqual(output.read_text(), f'PYTHONPATH={temporary}/support-msgq-python:/fixture/repository:/fixture/tools\n')

    def test_inherited_side_effects_are_source_repository_only(self):
        for file, job in [('sync.yml', 'sync'), ('naver-upstream-sync.yml', 'sync'), ('wiki-settings-publish.yaml', 'synchronize'), ('carrot-route-vault-publish.yaml', 'publish')]:
            with self.subTest(file=file):
                data = yaml.load((ROOT / '.github/workflows' / file).read_text(), Loader=yaml.BaseLoader)
                self.assertIn("github.repository == 'bin9208/openpilot'", data['jobs'][job].get('if', ''))

    def test_rust_checks_cover_push_and_protected_prs(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(data['on']['push']['branches'], ['**'])
        self.assertEqual(set(data['on']['pull_request']['branches']), {'dev', 'main'})
        for event in ['push', 'pull_request']:
            self.assertNotIn('paths', data['on'][event])
            self.assertNotIn('paths-ignore', data['on'][event])
        gate = data['jobs']['fast']
        self.assertEqual(gate['if'], '${{ always() }}')
        self.assertEqual(set(gate['needs']), {'model-memory', 'model-pipelines', 'logger-runtime', 'support-runtime', 'telemetry-runtime', 'startup-runtime', 'hardware-runtime', 'web-upload-timeouts'})
        validation = next(step for step in gate['steps'] if 'MEMORY' in step.get('env', {}))
        self.assertEqual(validation['env'], {'MEMORY': '${{ needs.model-memory.result }}', 'PIPELINES': '${{ needs.model-pipelines.result }}',
                                            'LOGGER': '${{ needs.logger-runtime.result }}',
                                            'SUPPORT': '${{ needs.support-runtime.result }}',
                                            'TELEMETRY': '${{ needs.telemetry-runtime.result }}',
                                            'STARTUP': '${{ needs.startup-runtime.result }}',
                                            'HARDWARE': '${{ needs.hardware-runtime.result }}',
                                            'UPLOAD_TIMEOUTS': '${{ needs.web-upload-timeouts.result }}'})
        results = dict.fromkeys(validation['env'], 'success')
        command = ['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', validation['run']]
        self.assertEqual(subprocess.run(command, env=results, capture_output=True).returncode, 0)
        for name in results:
            for result in ('failure', 'cancelled', 'skipped', ''):
                with self.subTest(job=name, result=result):
                    self.assertNotEqual(subprocess.run(command, env=results | {name: result}, capture_output=True).returncode, 0)
            with self.subTest(job=name, result='absent'):
                self.assertNotEqual(subprocess.run(command, env={key: value for key, value in results.items() if key != name},
                                                  capture_output=True).returncode, 0)
        for job in data['jobs'].values():
            self.assertNotIn('continue-on-error', job)

if __name__ == '__main__':
    unittest.main()

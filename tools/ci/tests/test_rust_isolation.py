"""Prevent inherited publishers from acting in the independent Rust repository."""
from pathlib import Path
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import tempfile
import tomllib
from types import SimpleNamespace
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]
LOCAL = ['self-hosted', 'Linux', 'X64', 'dev-md-local']
TRUST = ("github.repository == 'bin9208/openpilot-rust' && "
         "(github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)")
TRUST_IF = '${{ ' + TRUST + ' }}'
ARM_JOBS = ('webcam-runtime', 'planner-runtime', 'carrot-man-runtime', 'webrtc-runtime',
            'xiaoge-runtime', 'radar-arm', 'radard-runtime', 'carrot-navi-arm')

class RustIsolationTests(unittest.TestCase):
    def test_every_job_is_local_and_untrusted_source_never_schedules_it(self):
        for path in sorted((ROOT / '.github/workflows').glob('*')):
            data = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            # A privileged PR trigger needs a separate source-trust review.
            self.assertNotIn('pull_request_target', data['on'])
            for name, job in data['jobs'].items():
                with self.subTest(workflow=path.name, job=name):
                    if 'uses' not in job:
                        self.assertEqual(job.get('runs-on'), LOCAL)
                    self.assertIn(TRUST, job.get('if', ''))
                    for repository, event, head in (
                            ('bin9208/openpilot-rust', 'pull_request', 'contributor/fork'),
                            ('contributor/fork', 'push', ''),
                            ('contributor/fork', 'workflow_dispatch', '')):
                        github = SimpleNamespace(repository=repository, event_name=event,
                            ref='refs/pull/7/merge', event=SimpleNamespace(pull_request=SimpleNamespace(
                                head=SimpleNamespace(repo=SimpleNamespace(full_name=head)))))
                        expression = job['if'][3:-2].strip().replace('&&', ' and ').replace('||', ' or ')
                        self.assertFalse(eval(expression, {'__builtins__': {}}, {'github': github, 'always': lambda: True}))

    def test_untrusted_skips_never_publish_any_legacy_required_check_name(self):
        required = {
            ('fast-checks.yml', 'fast'): 'fast checks',
            ('integration.yml', 'gate'): 'integration gate',
            ('user-docs.yaml', 'check_user_docs'): 'check mapped user docs',
            ('rust.yml', 'fast'): 'rust checks',
            ('rust.yml', 'arm64'): 'rust aarch64 build',
        }
        contexts = (
            ('bin9208/openpilot-rust', 'push', '', True),
            ('bin9208/openpilot-rust', 'workflow_dispatch', '', True),
            ('bin9208/openpilot-rust', 'pull_request', 'bin9208/openpilot-rust', True),
            ('bin9208/openpilot-rust', 'pull_request', 'contributor/fork', False),
            ('contributor/fork', 'push', '', False),
            ('contributor/fork', 'pull_request', 'contributor/fork', False),
        )
        for (file, identifier), legacy in required.items():
            data = yaml.load((ROOT / '.github/workflows' / file).read_text(), Loader=yaml.BaseLoader)
            job = data['jobs'][identifier]
            self.assertIn(TRUST, job['if'])
            self.assertIn(TRUST, job['name'])
            for repository, event, head, trusted in contexts:
                with self.subTest(workflow=file, job=identifier, repository=repository, event=event, head=head):
                    github = SimpleNamespace(repository=repository, event_name=event,
                        event=SimpleNamespace(pull_request=SimpleNamespace(
                            head=SimpleNamespace(repo=SimpleNamespace(full_name=head)))))
                    expression = job['name'][3:-2].strip().replace('&&', ' and ').replace('||', ' or ')
                    actual = eval(expression, {'__builtins__': {}}, {'github': github})
                    if trusted:
                        self.assertEqual(actual, legacy)
                    else:
                        # A skipped check is accepted by GitHub. The exact
                        # required context must therefore remain unreported.
                        self.assertNotIn(actual, required.values())
                        self.assertIn('awaits trusted validation', actual)

    def test_arm_transport_retains_guest_execution_and_host_visible_evidence(self):
        jobs = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)['jobs']
        for name in ARM_JOBS:
            with self.subTest(job=name):
                job = jobs[name]
                self.assertIn('LOCAL_CI_ARCH', job['env'])
                self.assertEqual(job['defaults']['run']['shell'], 'openpilot-local-shell {0}')
                actions = [step for step in job['steps'] if step.get('uses') == './.github/actions/local-arm']
                self.assertEqual(len(actions), 2)
                self.assertNotIn('if', actions[0])
                self.assertEqual(actions[1]['if'], 'always()')
                self.assertEqual(actions[1]['with']['cleanup'], 'true')
                if name in ('webcam-runtime', 'webrtc-runtime'):
                    dependency = next(step for step in job['steps'] if step.get('name') == 'Install ARM inspection utility')
                    self.assertIn('if [ "$LOCAL_CI_ARCH" = aarch64 ]; then', dependency['run'])
                    self.assertIn('"$GITHUB_WORKSPACE/tools/ci/apt-native" install -y file', dependency['run'])
                for step in job['steps']:
                    if step.get('uses', '').startswith('actions/setup-python@'):
                        self.assertEqual(step['if'], "env.LOCAL_CI_ARCH == 'x86_64'")
                    if step.get('uses', '').startswith('actions/upload-artifact@'):
                        paths = step['with']['path']
                        self.assertNotIn('rust/target', paths)
                        self.assertIn('${{ env.CI_JOB_TEMP }}/local-arm/execution.json', paths)
                        self.assertIn('${{ env.CI_JOB_TEMP }}/local-arm/qemu-version.txt', paths)
                        if name == 'radard-runtime':
                            self.assertIn('${{ env.CI_JOB_TEMP }}/radard-native/bin/', paths)
                        else:
                            self.assertIn('${{ env.LOCAL_CI_TARGET_DIR }}', paths)

    def test_all_checked_out_jobs_initialize_and_collect_owned_temp_before_cleanup(self):
        for path in sorted((ROOT / '.github/workflows').glob('*')):
            self.assertNotIn('${{ runner.temp }}', path.read_text())
            data = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            for name, job in data['jobs'].items():
                steps = job.get('steps', [])
                checkout = [index for index, step in enumerate(steps) if step.get('uses', '').startswith('actions/checkout@')]
                if not checkout:
                    continue
                with self.subTest(workflow=path.name, job=name):
                    self.assertEqual(steps[checkout[0] + 1]['uses'], './.github/actions/job-temp')
                    self.assertNotIn('cleanup', steps[checkout[0] + 1].get('with', {}))
                    self.assertEqual(steps[-1]['uses'], './.github/actions/job-temp')
                    self.assertEqual(steps[-1]['if'], 'always()')
                    self.assertEqual(steps[-1]['with']['cleanup'], 'true')

    def test_repeated_jobs_leave_stale_runner_evidence_and_other_owners_untouched(self):
        action_path = ROOT / '.github/actions/job-temp/action.yml'
        self.assertTrue(action_path.is_file())
        action = yaml.load(action_path.read_text(), Loader=yaml.BaseLoader)['runs']['steps']
        with tempfile.TemporaryDirectory(prefix='persistent runner ') as temporary:
            root = Path(temporary)
            stale = root / 'diagnostic-close-red'
            stale.mkdir()
            (stale / 'retain').write_text('old evidence')
            outputs = []
            for attempt in range(2):
                command_file = root / f'github-env-{attempt}'
                environment = dict(os.environ, RUNNER_TEMP=str(root), GITHUB_ENV=str(command_file),
                    GITHUB_RUN_ID='497', GITHUB_RUN_ATTEMPT=str(attempt + 1), GITHUB_JOB='diagnostic-close')
                environment.pop('BASH_ENV', None)
                setup = subprocess.run(['/bin/bash', '-euc', action[0]['run']], env=environment,
                                       capture_output=True, text=True, timeout=10)
                self.assertEqual(setup.returncode, 0, setup.stderr)
                environment.update(dict(line.split('=', 1) for line in command_file.read_text().splitlines()))
                owned = Path(environment['CI_JOB_TEMP'])
                self.addCleanup(shutil.rmtree, owned, ignore_errors=True)
                outputs.append(owned)
                probe = subprocess.run(['/bin/bash', '-euc',
                    'mkdir "$RUNNER_TEMP/diagnostic-close-red"; printf evidence > "$RUNNER_TEMP/diagnostic-close-red/result"'],
                    env=environment, capture_output=True, text=True, timeout=10)
                self.assertEqual(probe.returncode, 0, probe.stderr)
                self.assertEqual((owned / 'diagnostic-close-red/result').read_text(), 'evidence')
                (owned / 'outside').symlink_to(root, target_is_directory=True)
                marker = (owned / 'owner').read_text()
                (owned / 'owner').write_text('another-job')
                refused = subprocess.run(['/bin/bash', '-euc', action[1]['run']], env=environment,
                                         capture_output=True, text=True, timeout=10)
                self.assertNotEqual(refused.returncode, 0)
                self.assertTrue(owned.is_dir())
                (owned / 'owner').write_text(marker)
                cleaned = subprocess.run(['/bin/bash', '-euc', action[1]['run']], env=environment,
                                         capture_output=True, text=True, timeout=10)
                self.assertEqual(cleaned.returncode, 0, cleaned.stderr)
                self.assertFalse(owned.exists())
                self.assertEqual((stale / 'retain').read_text(), 'old evidence')
            self.assertNotEqual(outputs[0], outputs[1])

    def test_neutral_helpers_match_exact_upstream_blobs_except_host_apt_delta(self):
        blobs = {
            '.github/actions/local-arm/action.yml': 'f51440bb48fd6c2d69b503eaf8601a36b103e062',
            '.github/actions/job-temp/action.yml': '154566f4016bc4a84a0650aacae93b847f694485',
            'tools/ci/local-arm': 'c8ae0f44d6ff3430cddf757239e4208835e96be9',
        }
        for name, expected in blobs.items():
            path = ROOT / name
            self.assertTrue(path.is_file(), name)
            source = path.read_bytes()
            if name == 'tools/ci/local-arm':
                self.assertIn(b'"$source_dir/tools/ci/apt-native" update', source)
                self.assertIn(b'"$source_dir/tools/ci/apt-native" install -y proot qemu-user-static', source)
                source = source.replace(b'"$source_dir/tools/ci/apt-native" update',
                    b'sudo -n apt-get -o APT::Architectures::=amd64 update')
                source = source.replace(b'"$source_dir/tools/ci/apt-native" install -y proot qemu-user-static',
                    b'sudo -n apt-get install -y proot qemu-user-static')
            digest = hashlib.sha1(b'blob ' + str(len(source)).encode() + b'\0' + source).hexdigest()
            self.assertEqual(digest, expected, name)

    def test_native_apt_calls_are_isolated_without_changing_foreign_arm_sources(self):
        foreign = []
        for path in sorted((ROOT / '.github/workflows').glob('*')):
            data = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            for name, job in data['jobs'].items():
                for step in job.get('steps', []):
                    for line in step.get('run', '').splitlines():
                        if line.lstrip().startswith('#'):
                            continue
                        if 'apt-get' in line:
                            with self.subTest(workflow=path.name, job=name, line=line):
                                self.assertEqual((path.name, name), ('rust.yml', 'arm64'))
                                self.assertIn('Dir::Etc::sourcelist="$PWD/.github/apt/startup-ui-cross.sources"', line)
                                self.assertIn('Dir::Etc::sourceparts=-', line)
                                self.assertNotIn('apt-native', line)
                                foreign.append(line)
                        if 'apt-native' in line:
                            for command in line.split('&&'):
                                arguments = shlex.split(command)
                                self.assertEqual(arguments[0], '$GITHUB_WORKSPACE/tools/ci/apt-native')
                                self.assertIn(arguments[1], ('update', 'install'))
                                self.assertFalse(any(':arm64' in argument for argument in arguments))
        self.assertEqual(len(foreign), 2)
        self.assertIn(' update', foreign[0])
        self.assertIn('install -y libegl-dev:arm64 libgles-dev:arm64 libdrm-dev:arm64 libgbm-dev:arm64', foreign[1])
        data = yaml.load((ROOT / '.github/workflows/fast-checks.yml').read_text(), Loader=yaml.BaseLoader)
        commands = '\n'.join(step.get('run', '') for step in data['jobs']['fast']['steps'])
        self.assertIn('python -m unittest discover -s tools/ci/tests -v', commands)

    def test_host_image_cleanup_never_runs_on_the_persistent_runner(self):
        with tempfile.TemporaryDirectory(prefix='host image cleanup ') as temporary:
            directory = Path(temporary)
            log = directory / 'sudo.log'
            sudo = directory / 'sudo'
            sudo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$CLEANUP_LOG"\n')
            sudo.chmod(0o755)
            cleanups = 0
            for path in sorted((ROOT / '.github/workflows').glob('*')):
                data = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
                for name, job in data['jobs'].items():
                    for step in job.get('steps', []):
                        lines = step.get('run', '').splitlines()
                        for index, line in enumerate(lines):
                            if 'sudo rm ' not in line or not any(root in line for root in ('/usr/local/', '/usr/share/', '/opt/')):
                                continue
                            cleanups += 1
                            # Execute the actual guard and removal command with
                            # a recording sudo, never the destructive operation.
                            fragment = '\n'.join(lines[max(0, index - 1):index + 2])
                            self.assertIn('if [ "${{ runner.environment }}" = github-hosted ]; then', fragment)
                            self.assertEqual(lines[index + 1].strip(), 'fi')
                            for environment in ('self-hosted', 'github-hosted'):
                                with self.subTest(workflow=path.name, job=name, environment=environment):
                                    log.unlink(missing_ok=True)
                                    subprocess.run(['/bin/bash', '--noprofile', '--norc', '-euc',
                                                    fragment.replace('${{ runner.environment }}', environment)],
                                                   env={'PATH': str(directory), 'CLEANUP_LOG': str(log)},
                                                   check=True, capture_output=True, timeout=10)
                                    if environment == 'self-hosted':
                                        self.assertFalse(log.exists())
                                    else:
                                        self.assertEqual(log.read_text(), ' '.join(shlex.split(line)[1:]) + '\n')
            self.assertGreater(cleanups, 0)

    def test_webcam_requires_two_architectures_and_real_capture_ipc(self):
        workflow = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = workflow['jobs']['webcam-runtime']
        self.assertEqual(job['strategy']['matrix']['runner'], ['ubuntu-24.04', 'ubuntu-24.04-arm'])
        self.assertEqual(job['if'], TRUST_IF)
        self.assertEqual(job['env']['CARGO_BUILD_JOBS'], '2')
        self.assertEqual(job['env']['CARGO_INCREMENTAL'], '0')
        self.assertEqual(job['env']['CARGO_PROFILE_DEV_DEBUG'], '0')
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('webcam_source_requirements.txt', 'build-xiaoge-opencv -- --prepare-only',
                         'build-webcam-opencv', 'build_visionipc_python.py', 'libswscale-dev=7:6.1.1-*',
                         'pkg-config --modversion libswscale', 'openpilot-webcam --features native',
                         'webcam_camera_compare.py', 'webcam_worker_compare.py', 'odd-height unaligned-even',
                         'webcam_cli_compare.py', 'webcam_capture_errors.py', 'webcam_capture_fault.cc'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python -P rust/tools/webcam_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)
        artifacts = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(artifacts), 1)
        self.assertEqual(artifacts[0]['if'], 'always()')
    def test_athena_requires_complete_native_runtime_and_codec_checks(self):
        workflow = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = workflow['jobs']['athena-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('openpilot-athena', 'openpilot-process-supervision', 'check_athena_runtime.py', 'check_jpeg_sanitizers.py'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'check_athena_runtime.py' in step.get('run', '') or 'check_jpeg_sanitizers.py' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)
        artifacts = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(artifacts), 1)
        self.assertEqual(artifacts[0]['if'], 'always()')

    def test_ui_connectivity_requires_source_render_and_private_protocol_checks(self) -> None:
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['ui-connectivity']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('xvfb-run', 'check_ui_framework.py', 'check_startup_ui.py',
                         'check_ui_emoji.py', 'check_ui_translations.py', 'check_wifi_policy.py',
                         'check_wifi_runtime.py', 'check_cweb_policy.py', 'check_cweb_http.py',
                         'check_cweb_daemon.py', 'check_cweb_address.py', 'build_params_python.py'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)
        artifacts = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(artifacts), 1)
        self.assertEqual(artifacts[0]['if'], 'always()')

    def test_estimators_require_original_models_loops_and_native_boundaries(self) -> None:
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['estimation-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('build_locationd_oracle.py', 'build_paramsd_oracle.py', 'build_params_python.py',
                         'check_locationd.py', 'check_locationd_loop.py', 'check_locationd_timestamp.py',
                         'check_locationd_daemon.py', 'check_locationd_startup.py', 'check_locationd_native.py',
                         'check_paramsd.py', 'check_paramsd_loop.py', 'check_paramsd_daemon.py',
                         'check_paramsd_startup.py', 'check_paramsd_native.py',
                         'check_lagd_numeric.py', 'check_lagd_loop.py', 'check_lagd_cache.py',
                         'check_lagd_packet.py', 'check_lagd_daemon.py', 'check_lagd_params_io.py',
                         'pocketfft/native/kernel_test.cc', '-fsanitize=address,undefined'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)
        packages = tomllib.loads((ROOT / 'uv.lock').read_text())['package']
        eigen = next(package for package in packages if package['name'] == 'eigen')
        source_commit = eigen['source']['git'].split('#')[-1]
        for name in ('workspace', 'arm64', 'estimation-runtime'):
            setup = '\n'.join(step.get('run', '') for step in data['jobs'][name]['steps'])
            self.assertIn('@' + source_commit + '#subdirectory=eigen', setup)
            self.assertIn('LOCATIOND_EIGEN_INCLUDE=', setup)
            self.assertIn('PARAMSD_EIGEN_INCLUDE=', setup)

    def test_gnss_requires_source_serial_and_daemon_checks(self) -> None:
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['gnss-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_ublox.py', 'check_pigeon.py', 'check_ublox_serial.py', 'check_ublox_daemons.py',
                         'check_qcomgps_reference.py', 'check_qcomgps_daemon.py', 'check_qcomgps_nmea.py',
                         'check_qcomgps_assistance.py', 'build_msgq_python.py'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)

    def test_sensor_audio_requires_original_and_live_native_checks(self) -> None:
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['sensor-audio']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_sensord.py', 'check_sensord_kernel.py', 'check_sensord_daemon.py',
                         'check_micd_analysis.py', 'check_micd_daemon.py', 'check_soundd.py', 'check_soundd_daemon.py', 'check_feedbackd.py',
                         'build_msgq_python.py', 'build_params_python.py', 'miri test'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)

    def test_startup_services_require_source_and_live_native_protocols(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['startup-services']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_lpa.py', 'check_bridge.py', 'check_agnos.py', 'build_bridge_reference.py', 'build_msgq_python.py'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', '') or 'check_bridge.py' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)

    def test_platform_runtime_requires_original_source_and_live_native_processes(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['platform-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_manager.py', 'check_manager_logging.py', 'generate_manager_cars.py',
                         'examples/manager_native', 'examples/manager_adapters',
                         'check_modem.py', 'check_modem_process_boundary.py',
                         'check_hardwared.py', 'check_hardwared_daemon.py'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'python rust/tools/check_' in step.get('run', ''):
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)

    def test_workspace_checks_run_without_waiting_for_runtime_jobs(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['workspace']
        self.assertNotIn('needs', job)
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('cargo fmt --all --check', 'cargo clippy --workspace', 'cargo test --workspace',
                         'cargo build --workspace --release --locked', 'check_usbgpu_hardware.py',
                         'check_usbgpu_asm.py', 'check_usbgpu_status.py'):
            self.assertIn(required, commands)
        # Exact trusted names and blocked-name separation are exercised by
        # test_untrusted_skips_never_publish_any_legacy_required_check_name.
        self.assertIn(TRUST, data['jobs']['fast']['name'])
        self.assertEqual(len(data['jobs']['fast']['steps']), 1)

    def test_hardware_runtime_requires_source_and_native_boundaries(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['hardware-runtime']
        self.assertEqual(job['if'], TRUST_IF)
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
        self.assertEqual(job['if'], TRUST_IF)
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

    def test_radar_requires_fresh_source_runtime_arm_and_memory_checks(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        for name in ('radar-runtime', 'radar-arm', 'radar-memory'):
            job = data['jobs'][name]
            with self.subTest(job=name):
                self.assertEqual(job['if'], TRUST_IF)
                self.assertNotIn('continue-on-error', job)
                commands = '\n'.join(step.get('run', '') for step in job['steps'])
                self.assertIn('check-disk-space', commands)
                self.assertEqual(job['env']['CARGO_INCREMENTAL'], '0')
                self.assertTrue(any(step.get('if') == 'always()' and
                                    step.get('uses', '').startswith('actions/upload-artifact@') for step in job['steps']))
        host = '\n'.join(step.get('run', '') for step in data['jobs']['radar-runtime']['steps'])
        for required in ('build_params_python.py', 'build_msgq_python.py', 'RADAR_PARAMS_BINDING',
                         'radarcan_prepare_assets.py', '--bin stage-torque-numerics', 'test_radarcan_ci.py',
                         'check_radarcan_ci.py --phase reference', 'check_radarcan_ci.py --phase ipc',
                         '-p openpilot-radarcan -p openpilot-can', '--all-targets --locked -- -D warnings'):
            self.assertIn(required, host)
        arm = '\n'.join(step.get('run', '') for step in data['jobs']['radar-arm']['steps'])
        self.assertEqual(data['jobs']['radar-arm']['runs-on'], LOCAL)
        for required in ('--target aarch64-unknown-linux-gnu', '--no-run', '--message-format=json',
                         '"$radar_test" --test-threads=1', 'test "$(cat "$RUNNER_TEMP/radar-arm/machine.txt")" = aarch64',
                         'check_radarcan_ci.py --phase reference', 'check_radarcan_ci.py --phase ipc',
                         '--platform manylinux_2_27_aarch64', 'sha256sum'):
            self.assertIn(required, arm)
        self.assertEqual(data['jobs']['radar-arm']['env']['LOCAL_CI_ARCH'], 'aarch64')
        self.assertEqual(data['jobs']['radar-arm']['defaults']['run']['shell'], 'openpilot-local-shell {0}')
        memory = '\n'.join(step.get('run', '') for step in data['jobs']['radar-memory']['steps'])
        for required in ('nightly-2026-09-29', 'numerics_', 'scheduler_argument_abi_preserves_source_fifo_priority',
                         '-Zmiri-no-extra-rounding-error', '-Zmiri-strict-provenance', '-Zmiri-symbolic-alignment-check',
                         '-Zmiri-preemption-rate=0.1', '-Zmiri-tree-borrows'):
            self.assertIn(required, memory)

    def test_selfdrived_requires_source_loop_native_ipc_and_failure_checks(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['selfdrive-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        for checker in ('check_selfdrived_controller.py', 'check_selfdrived_ipc.py', 'check_selfdrived_failures.py'):
            step = next(step for step in job['steps'] if checker in step.get('run', ''))
            self.assertNotIn('if', step)
            self.assertNotIn('continue-on-error', step)
        steps = '\n'.join(step.get('run', '') for step in job['steps'])
        self.assertIn('-p openpilot-selfdrived -p openpilot-messaging --locked', steps)
        self.assertIn('build_params_python.py', steps)
        self.assertIn('build_msgq_python.py', steps)
        self.assertTrue(any(step.get('if') == 'always()' and step.get('uses', '').startswith('actions/upload-artifact')
                            for step in job['steps']))

    def test_navigation_requires_original_transport_auth_and_timer_evidence(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['navd-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('build_params_python.py', 'build_msgq_python.py', 'NAVD_PARAMS_BINDING',
                         'requests==2.34.2', 'urllib3==2.7.0', 'PyJWT==2.14.0',
                         '-p openpilot-navd --features native --bins --examples --locked',
                         'check_navd_policy.py', 'check_navd_engine.py', 'check_navd_http.py', '--timeouts',
                         'check_navd_config.py', 'check_navd_destination.py', 'check_navd_ipc.py'):
            self.assertIn(required, commands)
        self.assertTrue(any(step.get('if') == 'always()' and
                            'navd-native/' in step.get('with', {}).get('path', '') for step in job['steps']))
        arm = '\n'.join(step.get('run', '') for step in data['jobs']['arm64']['steps'])
        self.assertIn('cargo build -p openpilot-navd --features native --bins --examples --release --locked --target aarch64-unknown-linux-gnu', arm)
        self.assertIn('release/openpilot-set-destination', arm)

    def test_inherited_side_effects_are_source_repository_only(self):
        for file, job in [('sync.yml', 'sync'), ('naver-upstream-sync.yml', 'sync'), ('wiki-settings-publish.yaml', 'synchronize'), ('carrot-route-vault-publish.yaml', 'publish')]:
            with self.subTest(file=file):
                data = yaml.load((ROOT / '.github/workflows' / file).read_text(), Loader=yaml.BaseLoader)
                self.assertIn("github.repository == 'bin9208/openpilot'", data['jobs'][job].get('if', ''))

    def test_rust_checks_cover_protected_pushes_and_prs(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(set(data['on']['push']['branches']), {'dev', 'main'})
        self.assertEqual(set(data['on']['pull_request']['branches']), {'dev', 'main'})
        for event in ['push', 'pull_request']:
            self.assertNotIn('paths', data['on'][event])
            self.assertNotIn('paths-ignore', data['on'][event])
        gate = data['jobs']['fast']
        self.assertEqual(gate['if'], '${{ ' + TRUST + ' && (always()) }}')
        self.assertEqual(set(gate['needs']), {
            'workspace', 'model-memory', 'model-pipelines', 'logger-runtime', 'support-runtime', 'telemetry-runtime',
            'startup-runtime', 'hardware-runtime', 'platform-runtime', 'startup-services', 'sensor-audio', 'gnss-runtime',
            'estimation-runtime', 'ui-connectivity', 'athena-runtime', 'controls-runtime', 'web-upload-timeouts', 'joystickd-runtime',
            'planner-runtime', 'planner-memory', 'webcam-runtime', 'webrtc-runtime',
            'card-runtime', 'radar-runtime', 'radar-arm', 'radar-memory', 'navd-runtime', 'radard-runtime', 'carrot-navi-runtime', 'carrot-navi-arm', 'selfdrive-runtime', 'panda-runtime', 'camera-runtime', 'encoder-runtime', 'xiaoge-runtime', 'xiaoge-memory', 'ui-runtime', 'carrot-man-runtime',
        })
        validation = next(step for step in gate['steps'] if 'MEMORY' in step.get('env', {}))
        self.assertEqual(validation['env'], {'WORKSPACE': '${{ needs.workspace.result }}', 'MEMORY': '${{ needs.model-memory.result }}',
                                            'PIPELINES': '${{ needs.model-pipelines.result }}',
                                            'LOGGER': '${{ needs.logger-runtime.result }}',
                                            'SUPPORT': '${{ needs.support-runtime.result }}',
                                            'TELEMETRY': '${{ needs.telemetry-runtime.result }}',
                                            'STARTUP': '${{ needs.startup-runtime.result }}',
                                            'HARDWARE': '${{ needs.hardware-runtime.result }}',
                                            'PLATFORM': '${{ needs.platform-runtime.result }}',
                                            'STARTUP_SERVICES': '${{ needs.startup-services.result }}',
                                            'SENSOR_AUDIO': '${{ needs.sensor-audio.result }}',
                                            'GNSS': '${{ needs.gnss-runtime.result }}',
                                            'ESTIMATION': '${{ needs.estimation-runtime.result }}',
                                            'UI_CONNECTIVITY': '${{ needs.ui-connectivity.result }}',
                                            'ATHENA': '${{ needs.athena-runtime.result }}',
                                            'CONTROLS': '${{ needs.controls-runtime.result }}',
                                            'UPLOAD_TIMEOUTS': '${{ needs.web-upload-timeouts.result }}',
                                            'JOYSTICKD': '${{ needs.joystickd-runtime.result }}',
                                            'PLANNER': '${{ needs.planner-runtime.result }}',
                                            'PLANNER_MEMORY': '${{ needs.planner-memory.result }}',
                                            'CARD': '${{ needs.card-runtime.result }}',
                                            'RADAR': '${{ needs.radar-runtime.result }}',
                                            'RADAR_ARM': '${{ needs.radar-arm.result }}',
                                            'RADAR_MEMORY': '${{ needs.radar-memory.result }}',
                                            'NAVD': '${{ needs.navd-runtime.result }}',
                                            'RADARD': '${{ needs.radard-runtime.result }}',
                                            'CARROT_NAVI': '${{ needs.carrot-navi-runtime.result }}',
                                            'CARROT_NAVI_ARM': '${{ needs.carrot-navi-arm.result }}',
                                            'SELFDRIVE': '${{ needs.selfdrive-runtime.result }}',
                                            'PANDA': '${{ needs.panda-runtime.result }}',
                                            'CARROT_MAN': '${{ needs.carrot-man-runtime.result }}',
                                            'WEBRTC': '${{ needs.webrtc-runtime.result }}',
                                            'WEBCAM': '${{ needs.webcam-runtime.result }}',
                                            'CAMERA': '${{ needs.camera-runtime.result }}',
                                            'ENCODER': '${{ needs.encoder-runtime.result }}',
                                            'XIAOGE': '${{ needs.xiaoge-runtime.result }}',
                                            'XIAOGE_MEMORY': '${{ needs.xiaoge-memory.result }}',
                                            'UI_RUNTIME': '${{ needs.ui-runtime.result }}'})
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

    def test_webrtc_requires_native_host_and_arm_real_media_and_failure_boundaries(self):
        jobs = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)['jobs']
        job = jobs['webrtc-runtime']
        self.assertEqual(job['strategy']['matrix']['runner'], ['ubuntu-24.04', 'ubuntu-24.04-arm'])
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('webrtc_source_requirements.txt', 'libavcodec-dev=7:6.1.1-*', 'build_params_python.py', 'build_msgq_python.py',
                         '-p openpilot-webrtc --features native --bins', '--features native --lib --locked', '--features native --all-targets --locked',
                         'webrtc_stun_checks', 'webrtc_identifiers_compare.py', 'webrtc_multicamera_compare.py', 'webrtc_fir_compare.py',
                         'webrtc_body_compare.py', 'webrtc_params_compare.py', 'webrtc_cli_compare.py', 'webrtc_srflx_compare.py',
                         'webrtc_debug_compare.py', 'source-standard-h264 native-standard-h264',
                         'WEBRTC_PARAMS_BINDING', 'check-disk-space', 'SHA256SUMS'):
            self.assertIn(required, commands)
        for step in job['steps']:
            if 'run' in step:
                self.assertNotIn('if', step)
                self.assertNotIn('continue-on-error', step)
        artifacts = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(artifacts), 1)
        self.assertEqual(artifacts[0]['if'], 'always()')
        requirements = (ROOT / 'rust/tools/webrtc_source_requirements.txt').read_text().splitlines()
        self.assertIn('aiortc==1.14.0', requirements)
        self.assertIn('av==16.1.0', requirements)
        self.assertIn('aioice==0.10.2', requirements)

    def test_xiaoge_requires_both_architectures_and_instrumented_native_libraries(self):
        jobs = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)['jobs']
        self.assertEqual(jobs['xiaoge-runtime']['strategy']['matrix']['runner'], ['ubuntu-24.04', 'ubuntu-24.04-arm'])
        required = {
            'xiaoge-runtime': ('build-xiaoge-opencv', 'build_params_python.py', 'build_visionipc_python.py',
                              'check_xiaoge_ci.py', '--features native-skip-miri', 'test_xiaoge_ci.py'),
            'xiaoge-memory': ('--sanitizers address,undefined', 'check_xiaoge_native_memory.py',
                             '-Zmiri-strict-provenance', '-Zmiri-symbolic-alignment-check',
                             '-Zmiri-preemption-rate=0.1', '-Zmiri-tree-borrows', '--no-default-features'),
        }
        for name, commands in required.items():
            job = jobs[name]
            self.assertEqual(job['if'], TRUST_IF)
            script = '\n'.join(step.get('run', '') for step in job['steps'])
            for command in commands:
                self.assertIn(command, script)
            for step in job['steps']:
                if 'run' in step:
                    self.assertNotIn('if', step)
                    self.assertNotIn('continue-on-error', step)
            artifact = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
            self.assertEqual(len(artifact), 1)
            self.assertEqual(artifact[0]['if'], 'always()')

    def test_encoder_requires_source_runtime_and_pinned_arm_artifacts(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['encoder-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('build_visionipc_python.py', 'check_encoder_ci.py', 'test_encoder_outcomes.py',
                         '-p openpilot-dev-tools --locked --test encoder_stage', '--features native --bins --examples',
                         '--features openpilot-encoderd/native', '--message-format=json', 'zstandard==0.25.0'):
            self.assertIn(required, commands)
        check = next(step for step in job['steps'] if 'check_encoder_ci.py' in step.get('run', ''))
        self.assertNotIn('if', check)
        self.assertNotIn('continue-on-error', check)
        setup = next(step for step in job['steps'] if step.get('name') == 'Configure original encoder IPC imports')
        with tempfile.TemporaryDirectory(prefix='encoder IPC env ') as temporary:
            output = Path(temporary) / 'environment'
            environment = {'RUNNER_TEMP': temporary, 'PYTHONPATH': '/fixture/repository:/fixture/tools', 'GITHUB_ENV': str(output)}
            subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', setup['run']], env=environment, check=True)
            self.assertEqual(output.read_text(), f'PYTHONPATH={temporary}/encoder-native/python:/fixture/repository:/fixture/tools\n'
                                                f'ENCODER_MSGQ_PYTHON={temporary}/encoder-native/python\n')
        artifacts = [step for step in job['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(artifacts), 1)
        self.assertEqual(artifacts[0]['if'], 'always()')
        arm = next(step for step in data['jobs']['arm64']['steps']
                   if step.get('name') == 'Build encoder daemon and probes with pinned ARM codecs and ION')
        self.assertEqual(arm['working-directory'], 'rust')
        self.assertIn('--features static-ffmpeg,visionipc-ion', arm['run'])
        self.assertNotIn('if', arm)
        self.assertNotIn('continue-on-error', arm)
        self.assertIn('ENCODER_ARM_FFMPEG', arm['env']['FFMPEG_DIR'])
        self.assertIn('ENCODER_ARM_LIBYUV', arm['env']['ENCODER_LIBYUV_LIB'])

    def test_camera_requires_source_driver_fixture_and_visionipc_checks(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['camera-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_camerad_ci.py', 'build_visionipc_python.py', 'libclang-rt-18-dev',
                         'openpilot-camerad-runtime --features native-skip-miri', '--test vision --test vision_server'):
            self.assertIn(required, commands)
        step = next(step for step in job['steps'] if 'check_camerad_ci.py' in step.get('run', ''))
        self.assertNotIn('if', step)
        self.assertNotIn('continue-on-error', step)
        self.assertTrue(any(step.get('if') == 'always()' and step.get('uses', '').startswith('actions/upload-artifact')
                            for step in job['steps']))
        arm = next(step['run'] for step in data['jobs']['arm64']['steps'] if step.get('name') == 'Build generic and ION camera aarch64 artifacts')
        self.assertLess(arm.index('cp target/'), arm.index('--features visionipc-ion'))
        self.assertIn('--features native-skip-miri', arm)
        self.assertIn('sha256sum', arm)

    def test_panda_requires_native_transports_and_source_composition(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['panda-runtime']
        self.assertEqual(job['if'], TRUST_IF)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('check_pandad_ci.py', 'build_msgq_python.py', 'libusb1==3.4.0',
                         '-p openpilot-pandad -p openpilot-panda-usb -p openpilot-panda-spi -p openpilot-panda-spi-linux',
                         '--message-format=json', '--build-messages'):
            self.assertIn(required, commands)
        step = next(step for step in job['steps'] if 'check_pandad_ci.py' in step.get('run', ''))
        self.assertNotIn('if', step)
        self.assertNotIn('continue-on-error', step)
        self.assertTrue(any(step.get('if') == 'always()' and step.get('uses', '').startswith('actions/upload-artifact')
                            for step in job['steps']))

if __name__ == '__main__':
    unittest.main()

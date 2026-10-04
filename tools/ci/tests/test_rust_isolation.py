"""Prevent inherited publishers from acting in the independent Rust repository."""
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]

class RustIsolationTests(unittest.TestCase):
    def test_athena_requires_complete_native_runtime_and_codec_checks(self):
        workflow = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = workflow['jobs']['athena-runtime']
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('cargo fmt --all --check', 'cargo clippy --workspace', 'cargo test --workspace',
                         'cargo build --workspace --release --locked'):
            self.assertIn(required, commands)
        self.assertEqual(data['jobs']['fast']['name'], 'rust checks')
        self.assertEqual(len(data['jobs']['fast']['steps']), 1)

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
        for name, step_name, binding in (
            ('support-runtime', 'Configure original support IPC imports', 'support-msgq-python'),
            ('card-runtime', 'Configure original Card IPC imports', 'card-native/msgq'),
            ('selfdrive-runtime', 'Configure original selfdrived IPC imports', 'selfdrived-native/msgq'),
            ('camera-runtime', 'Configure original camera IPC imports', 'camera-native/python'),
            ('panda-runtime', 'Configure original Panda IPC imports', 'panda-native/msgq'),
            ('ui-runtime', 'Configure original UI IPC imports', 'ui-native/python'),
        ):
            setup = next(step for step in data['jobs'][name]['steps'] if step.get('name') == step_name)
            with self.subTest(job=name), tempfile.TemporaryDirectory(prefix='IPC env ') as temporary:
                output = Path(temporary) / 'environment'
                environment = {'RUNNER_TEMP': temporary, 'PYTHONPATH': '/fixture/repository:/fixture/tools', 'GITHUB_ENV': str(output)}
                subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', setup['run']], env=environment, check=True)
                self.assertEqual(output.read_text(), f'PYTHONPATH={temporary}/{binding}:/fixture/repository:/fixture/tools\n')

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
        self.assertEqual(set(gate['needs']), {
            'workspace', 'model-memory', 'model-pipelines', 'logger-runtime', 'support-runtime', 'telemetry-runtime',
            'startup-runtime', 'hardware-runtime', 'platform-runtime', 'startup-services', 'sensor-audio', 'gnss-runtime',
            'estimation-runtime', 'ui-connectivity', 'athena-runtime', 'controls-runtime', 'web-upload-timeouts',
            'card-runtime', 'selfdrive-runtime', 'camera-runtime', 'panda-runtime', 'ui-runtime', 'encoder-runtime', 'navd-runtime',
            'xiaoge-runtime', 'xiaoge-memory',
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
                                            'CARD': '${{ needs.card-runtime.result }}',
                                            'SELFDRIVE': '${{ needs.selfdrive-runtime.result }}',
                                            'CAMERA': '${{ needs.camera-runtime.result }}',
                                            'PANDA': '${{ needs.panda-runtime.result }}',
                                            'UI_RUNTIME': '${{ needs.ui-runtime.result }}',
                                            'ENCODER': '${{ needs.encoder-runtime.result }}',
                                            'NAVD': '${{ needs.navd-runtime.result }}',
                                            'XIAOGE': '${{ needs.xiaoge-runtime.result }}',
                                            'XIAOGE_MEMORY': '${{ needs.xiaoge-memory.result }}'})
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

    def test_xiaoge_requires_both_architectures_and_instrumented_native_libraries(self):
        jobs = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)['jobs']
        self.assertEqual(jobs['xiaoge-runtime']['strategy']['matrix']['runner'], ['ubuntu-24.04', 'ubuntu-24.04-arm'])
        required = {
            'xiaoge-runtime': ('build_xiaoge_opencv.py', 'build_params_python.py', 'build_visionipc_python.py',
                              'check_xiaoge_ci.py', '--features native-skip-miri', 'test_xiaoge_ci.py'),
            'xiaoge-memory': ('--sanitizers address,undefined', 'check_xiaoge_native_memory.py',
                             '-Zmiri-strict-provenance', '-Zmiri-symbolic-alignment-check',
                             '-Zmiri-preemption-rate=0.1', '-Zmiri-tree-borrows', '--no-default-features'),
        }
        for name, commands in required.items():
            job = jobs[name]
            self.assertNotIn('if', job)
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

    def test_navigation_requires_original_transport_auth_and_timer_evidence(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['navd-runtime']
        self.assertNotIn('if', job)
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

    def test_encoder_requires_source_runtime_and_pinned_arm_artifacts(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['encoder-runtime']
        self.assertNotIn('if', job)
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        for required in ('build_visionipc_python.py', 'check_encoder_ci.py', 'test_encoder_outcomes.py',
                         'test_encoder_target_stage.py', '--features native --bins --examples',
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

    def test_selfdrived_requires_source_loop_native_ipc_and_failure_checks(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['selfdrive-runtime']
        self.assertNotIn('if', job)
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

    def test_camera_requires_source_driver_fixture_and_visionipc_checks(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        job = data['jobs']['camera-runtime']
        self.assertNotIn('if', job)
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
        self.assertNotIn('if', job)
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

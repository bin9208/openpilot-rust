"""Fail-closed sequential CI, event routing and release dispatch contracts."""
from pathlib import Path
import ast
import json
import os
import shlex
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


def workflow(name='ci.yml'):
  return yaml.load((ROOT / '.github/workflows' / name).read_text(), Loader=yaml.BaseLoader)


def javascript(script, fixture):
  harness = '''
const fixture = JSON.parse(process.env.FIXTURE);
const outputs = {};
const context = {eventName: fixture.event, ref: fixture.ref, repo: {owner:'bin9208',repo:'openpilot-rust'}};
const github = {rest:{pulls:{list:'pulls'}}, paginate: async () => fixture.pulls || []};
const core = {setOutput: (key,value) => outputs[key] = value};
(async () => { SCRIPT; process.stdout.write(JSON.stringify(outputs)); })().catch(e => {console.error(e);process.exit(1);});
'''.replace('SCRIPT', script)
  return subprocess.run(['node', '-e', harness], env={**os.environ, 'FIXTURE': json.dumps(fixture)},
                        capture_output=True, text=True, check=True)


class WorkflowPolicyTests(unittest.TestCase):
  def test_one_entry_point_without_path_skips(self):
    events = workflow()['on']
    self.assertEqual(events['push']['branches'], ['dev', 'dev-md', 'main'])
    self.assertEqual(set(events['pull_request']['branches']), {'dev', 'dev-md', 'main'})
    self.assertIn('edited', events['pull_request']['types'])
    for event in ('push', 'pull_request'):
      self.assertNotIn('paths', events[event])
      self.assertNotIn('paths-ignore', events[event])
    for old in ('fast-checks.yml', 'integration.yml', 'rust.yml', 'user-docs.yaml',
                'tests.yaml', 'jetlink.yml', 'jetlink-native.yml', 'naver-ci.yml', 'naver-apk-tools.yml'):
      self.assertFalse((ROOT / '.github/workflows' / old).exists(), old)

  def test_parallel_pool_has_a_separate_preserving_concurrency_group(self):
    concurrency = workflow()['concurrency']
    self.assertEqual(concurrency, {
      'group': 'ci-${{ github.event_name }}-${{ github.event.pull_request.number || github.ref }}-parallel-v2',
      'cancel-in-progress': 'false',
      'queue': 'max',
    })
    self.assertNotEqual(concurrency['group'],
                        'ci-${{ github.event_name }}-${{ github.event.pull_request.number || github.ref }}')

  def test_authorized_release_write_group_is_not_split_by_validation_pool(self):
    self.assertEqual(workflow('dev-md-runtime.yml')['concurrency'], {
      'group': 'dev-md-runtime-${{ github.ref }}',
      'cancel-in-progress': 'false',
    })


  def test_independent_runtime_jobs_are_parallel_and_all_results_remain_required(self):
    jobs = workflow()['jobs']
    self.assertNotIn('needs', jobs['check_user_docs'])
    self.assertEqual(jobs['check_user_docs']['if'], "needs.route.outputs.full == 'true'")
    runtime = list(jobs)[list(jobs).index('workspace'):list(jobs).index('fast')]
    self.assertGreater(len(runtime), 30)
    for key in runtime:
      self.assertEqual(jobs[key]['needs'], ['check_user_docs'], key)
      self.assertIn(key, jobs['fast']['needs'])
      self.assertIn(key, jobs['arm64']['needs'])
      self.assertNotIn('continue-on-error', jobs[key])
      strategy = jobs[key].get('strategy', {})
      if 'matrix' in strategy:
        self.assertEqual(strategy['max-parallel'], '2')
        self.assertEqual(strategy['fail-fast'], 'true')
    self.assertEqual(jobs['runtime-delivery']['needs'], 'arm64')
    self.assertEqual(jobs['arm-build']['needs'], ['fast'])
    for key in ('build_release', 'model_web', 'model', 'android-core', 'jetlink-native'):
      self.assertEqual(jobs[key]['needs'], ['check_user_docs'])
      self.assertIn(key, jobs['integration-gate']['needs'])

  def test_gates_reject_failure_cancellation_skips_and_missing_results(self):
    jobs = workflow()['jobs']
    for key in ('integration-gate', 'fast', 'arm64'):
      job = jobs[key]
      self.assertIn('always()', job['if'])
      step = job['steps'][0]
      results = {name: {'result': 'success'} for name in job['needs']}
      def invoke(values):
        environment = {**os.environ, 'RESULTS': json.dumps(values), 'EXPECTED': step['env']['EXPECTED']}
        return subprocess.run(['bash', '-eu', '-c', step['run']], env=environment, capture_output=True).returncode
      self.assertEqual(invoke(results), 0, key)
      for dependency in results:
        for state in ('failure', 'cancelled', 'skipped', ''):
          with self.subTest(gate=key, dependency=dependency, state=state):
            self.assertNotEqual(invoke(results | {dependency: {'result': state}}), 0)
        self.assertNotEqual(invoke({name: value for name, value in results.items() if name != dependency}), 0)

  def test_cancelled_validation_never_starts_the_expensive_arm_build(self):
    jobs = workflow()['jobs']
    self.assertEqual(jobs['arm-build']['needs'], ['fast'])
    self.assertNotIn('if', jobs['arm-build'])
    self.assertIn('arm-build', jobs['arm64']['needs'])
    self.assertIn('always()', jobs['arm64']['if'])
    for key, job in jobs.items():
      if key not in ('integration-gate', 'fast', 'arm64'):
        self.assertNotIn('always()', job.get('if', ''), key)



  def test_dev_md_gate_still_rejects_dev_target(self):
    steps = workflow()['jobs']['check_user_docs']['steps']
    gate = next(step for step in steps if step.get('name') == 'Keep dev-md changes out of dev')
    for target, expected in [('dev', 1), ('dev-md', 0), ('main', 0), ('codex/feat-test', 0)]:
      result = subprocess.run(['bash', '-eu', '-c', gate['run']],
                              env={**os.environ, 'TARGET_BRANCH': target}, capture_output=True)
      self.assertEqual(result.returncode, expected)

  def test_android_apk_checks_have_an_explicit_source_matched_sdk(self):
    steps = workflow()['jobs']['android-core']['steps']
    setup = next(i for i, step in enumerate(steps) if step.get('uses', '').startswith('android-actions/setup-android@'))
    install = next(i for i, step in enumerate(steps) if 'sdkmanager' in step.get('run', ''))
    build = next(i for i, step in enumerate(steps) if ':app:assembleDebug' in step.get('run', ''))
    self.assertLess(setup, install)
    self.assertLess(install, build)
    app = (ROOT / 'tools/jetlink_android/app/build.gradle.kts').read_text()
    self.assertIn('compileSdk = 35', app)
    self.assertIn('buildToolsVersion = "35.0.0"', app)
    self.assertIn("'platforms;android-35' 'build-tools;35.0.0'", steps[install]['run'])
    reserve = next(step['run'] for step in steps[:setup] if 'ANDROID_HOME=' in step.get('run', ''))
    self.assertIn('free_bytes < 4 * 1024 * 1024 * 1024', reserve)
    self.assertIn('ANDROID_SDK_ROOT=', reserve)

  def test_user_selected_local_runner_policy_preserves_validation_scope(self):
    for name in ('ci.yml', 'dev-md-runtime.yml'):
      for key, job in workflow(name)['jobs'].items():
        selected = job['runs-on']
        matrix_route = '${{ matrix.runner == \'ubuntu-24.04-arm\' && \'ubuntu-24.04-arm\' || fromJSON(\'["self-hosted","Linux","X64","openpilot-rust"]\') }}'
        self.assertTrue(selected in ('ubuntu-24.04', 'ubuntu-24.04-arm', matrix_route) or
                        selected == ['self-hosted', 'Linux', 'X64', 'openpilot-rust'], (name, key))
    commands = '\n'.join(step.get('run', '') for step in workflow()['jobs']['check_user_docs']['steps'])
    self.assertNotIn('package.py source-policy', commands)
    delivery = '\n'.join(step.get('run', '') for step in workflow('dev-md-runtime.yml')['jobs']['package']['steps'])
    self.assertIn('package.py source-policy', delivery)
    native = '\n'.join(step.get('run', '') for step in workflow()['jobs']['xiaoge-runtime']['steps'])
    self.assertIn('--features native-skip-miri --all-targets', native)
    self.assertTrue((ROOT / 'rust/crates/xiaoge/tests/cadence.rs').is_file())
    self.assertTrue((ROOT / 'rust/crates/xiaoge/tests/camera_pixels.rs').is_file())

  def test_carrot_navi_build_uses_arm_guest_and_preserves_target_inspection(self):
    job = workflow()['jobs']['carrot-navi-arm']
    self.assertEqual(job['env']['LOCAL_CI_ARCH'], 'aarch64')
    self.assertEqual(job['env']['CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER'], 'gcc')
    self.assertEqual(job['defaults']['run']['shell'], 'openpilot-local-shell {0}')
    commands = '\n'.join(step.get('run', '') for step in job['steps'])
    self.assertIn('test "$(uname -m)" = aarch64', commands)
    self.assertIn('--target aarch64-unknown-linux-gnu --release --locked', commands)
    self.assertIn('readelf -h', commands)
    self.assertIn('sha256sum', commands)

  def test_arm_graphics_dependencies_do_not_change_persistent_host_packages(self):
    commands = '\n'.join(step.get('run', '') for step in workflow()['jobs']['arm-build']['steps'])
    self.assertNotIn('dpkg --add-architecture', commands)
    self.assertIn('Dir::Etc::parts "-";', commands)
    self.assertIn('Dir::State::status "$cross_root/state/status";', commands)
    self.assertIn('Dir::Cache "$cross_root/cache";', commands)
    self.assertIn('APT_CONFIG="$cross_root/apt.conf" apt-get --download-only', commands)
    self.assertIn('dpkg-deb --extract "$deb" "$cross_root/sysroot"', commands)
    self.assertIn('aarch64-linux-gnu-g++ --sysroot=%q', commands)
    self.assertIn('--library-directory "$cross_root/sysroot/usr/lib/aarch64-linux-gnu"', commands)

  def test_arm_graphics_apt_resolves_only_private_state_and_arm_target(self):
    steps = workflow()['jobs']['arm-build']['steps']
    script = next(step['run'] for step in steps if 'cross_root=$(mktemp' in step.get('run', ''))
    template = script.split('cat > "$cross_root/apt.conf" <<EOF\n', 1)[1].split('\nEOF', 1)[0]
    with tempfile.TemporaryDirectory(prefix='apt-private-') as temporary:
      config = Path(temporary) / 'apt.conf'
      config.write_text(template.replace('$cross_root', temporary).replace('$PWD', str(ROOT)))
      environment = dict(os.environ, APT_CONFIG=str(config))
      result = subprocess.run(['apt-config', 'shell', 'arch', 'APT::Architecture',
                               'state', 'Dir::State/d', 'status', 'Dir::State::status/f',
                               'cache', 'Dir::Cache/d', 'parts', 'Dir::Etc::parts',
                               'main', 'Dir::Etc::main'], env=environment,
                              capture_output=True, text=True, check=True)
      values = dict(line.split('=', 1) for line in shlex.split(result.stdout))
      self.assertEqual(values, dict(arch='arm64', state=temporary + '/state/',
                                   status=temporary + '/state/status', cache=temporary + '/cache/',
                                   parts='-', main='-'))

  def test_runtime_delivery_is_serial_and_requires_a_marked_merge(self):
    delivery = workflow()['jobs']['runtime-delivery']
    self.assertIn("'[runtime-package]'", delivery['if'])
    self.assertIn("refs/heads/dev-md", delivery['if'])
    self.assertIn("github.event_name == 'push'", delivery['if'])
    script = delivery['steps'][0]['with']['script']
    self.assertIn('pr.merged_at && pr.merge_commit_sha === context.sha', script)
    runtime = workflow('dev-md-runtime.yml')
    self.assertEqual(set(runtime['on']), {'workflow_dispatch'})
    self.assertEqual(runtime['jobs']['package']['needs'], 'verify')
    self.assertEqual(runtime['jobs']['publish']['needs'], ['verify', 'package'])
    self.assertNotIn('codex/fix-402-', (ROOT / '.github/workflows/dev-md-runtime.yml').read_text())
    for name in ('package', 'publish'):
      checkout = next(step for step in runtime['jobs'][name]['steps'] if 'actions/checkout@' in step.get('uses', ''))
      self.assertEqual(checkout['with']['ref'], '${{ needs.verify.outputs.source_sha }}')


class NativeAptSourceViewTests(unittest.TestCase):
  @classmethod
  def setUpClass(cls):
    cls.text = (ROOT/'tools/ci/native-apt-shell').read_text()
    code = cls.text.split("<<'PY_APT_SOURCES'\n", 1)[1].split('\nPY_APT_SOURCES', 1)[0]
    tree = ast.parse(code)
    pure = [n for n in tree.body if isinstance(n, (ast.Import, ast.ImportFrom, ast.FunctionDef))]
    cls.functions = {}
    exec(compile(ast.Module(body=pure, type_ignores=[]), '<actual-APT-source-parsers>', 'exec'), cls.functions)

  def test_canonical_deb822_and_multiarch_ports_keep_signature_and_architecture_fields(self):
    archive = ('Types: deb\nURIs: https://archive.ubuntu.com/ubuntu/\n'
               'Suites: noble noble-updates\nComponents: main universe\n'
               'Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\nArchitectures: amd64\n')
    ports = archive.replace('archive.ubuntu.com/ubuntu', 'ports.ubuntu.com/ubuntu-ports').replace('amd64', 'arm64')
    self.assertTrue(self.functions['official_deb822'](archive))
    self.assertTrue(self.functions['official_deb822'](ports))
    ppa = archive.replace('archive.ubuntu.com/ubuntu', 'ppa.launchpadcontent.net/appimagelauncher-team/stable/ubuntu')
    self.assertFalse(self.functions['official_deb822'](ppa))
    with self.assertRaises(ValueError):
      self.functions['official_deb822'](archive+'Trusted: yes\n')
    with self.assertRaises(ValueError):
      self.functions['official_deb822'](archive.replace('https://archive.ubuntu.com/ubuntu/',
        'https://archive.ubuntu.com/ubuntu/ https://ppa.launchpadcontent.net/example/stable/ubuntu/'))

  def test_legacy_sources_and_ports_are_selected_without_accepting_private_or_untrusted_sources(self):
    source = 'deb [arch=arm64 signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] https://ports.ubuntu.com/ubuntu-ports noble main'
    self.assertTrue(self.functions['official_legacy'](source))
    self.assertTrue(self.functions['official_legacy']('deb https://security.ubuntu.com/ubuntu noble-security main'))
    self.assertFalse(self.functions['official_legacy']('deb https://ppa.launchpadcontent.net/example/stable/ubuntu noble main'))
    self.assertFalse(self.functions['official_legacy']('deb https://archive.ubuntu.com.example/ubuntu noble main'))
    with self.assertRaises(ValueError):
      self.functions['official_legacy'](source.replace('arch=arm64', 'trusted=yes arch=arm64'))

  def test_native_host_detection_does_not_reenter_path_uname_from_bash_env(self):
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      called = root/'unexpected-uname'
      fake = root/'uname'
      fake.write_text('#!/usr/bin/env bash\nprintf called > '+shlex.quote(str(called))+'\nexit 99\n')
      fake.chmod(0o755)
      environment = dict(os.environ, BASH_ENV=str(ROOT/'tools/ci/native-apt-shell'),
                         PATH=str(root)+':'+os.environ['PATH'])
      result = subprocess.run(['/usr/bin/bash', '-eu', '-c', ':'], env=environment,
                              capture_output=True, text=True, timeout=10)
      self.assertEqual(result.returncode, 0, result.stderr)
      self.assertFalse(called.exists())


if __name__ == '__main__':
  unittest.main()

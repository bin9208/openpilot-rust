"""Regression guard for required-check routing and fail-closed aggregation."""
from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


def workflow(name):
  return yaml.load((ROOT / '.github/workflows' / name).read_text(), Loader=yaml.BaseLoader)


class WorkflowPolicyTests(unittest.TestCase):

  def test_integration_runs_before_and_after_merge_without_path_skips(self):
    spec = workflow('integration.yml')
    for event in ('push', 'pull_request'):
      self.assertEqual(spec['on'][event], {'branches': ['dev', 'main']})
    self.assertEqual(spec['jobs']['build']['with']['run_number'], '1')
    gate = spec['jobs']['gate']
    self.assertEqual(set(gate['needs']), {'build', 'navigation', 'apk', 'jetlink'})
    self.assertEqual(gate['if'], '${{ always() }}')
    step = gate['steps'][0]
    for name in ('BUILD', 'NAVIGATION', 'APK', 'JETLINK'):
      self.assertIn(f'test "${name}" = success', step['run'])
    for job in ('build', 'navigation', 'apk', 'jetlink'):
      self.assertNotIn('if', spec['jobs'][job])
      self.assertNotIn('continue-on-error', spec['jobs'][job])
      called = Path(spec['jobs'][job]['uses']).name
      self.assertIn('workflow_call', workflow(called)['on'])

  def test_documentation_check_runs_on_protected_branch_pushes(self):
    events = workflow('user-docs.yaml')['on']
    self.assertTrue({'main', 'dev'}.issubset(events['push']['branches']))
    self.assertIn('pull_request', events)

  def test_integration_execution_uses_owned_self_hosted_jobs(self):
    suites = {
      'tests.yaml': {'git_update', 'build_release', 'model_web'},
      'naver-ci.yml': {'navigation-ui', 'phone-mapping-contract', 'runtime-controller',
                       'wire-schema', 'naver-input', 'infrastructure'},
      'naver-apk-tools.yml': {'host-toolkit', 'payload'},
      'jetlink.yml': {'model', 'android-core'},
    }
    trusted = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) }}"
    for name, expected in suites.items():
      jobs = workflow(name)['jobs']
      self.assertEqual(set(jobs), expected)
      for job_id, job in jobs.items():
        with self.subTest(workflow=name, job=job_id):
          self.assertEqual(job['runs-on'], ['self-hosted', 'Linux', 'X64', 'openpilot-rust'])
          self.assertEqual(job['if'], trusted)
          self.assertNotIn('continue-on-error', job)
          steps = job['steps']
          self.assertEqual(steps[1]['uses'], './.github/actions/job-temp')
          self.assertEqual(steps[2]['uses'], './.github/actions/rust-path')
          self.assertEqual(steps[-1], {'uses': './.github/actions/job-temp',
                                      'if': 'always()', 'with': {'cleanup': 'true'}})

  def test_light_required_checks_remain_hosted(self):
    for name, job_id in (('integration.yml', 'gate'),
                         ('user-docs.yaml', 'check_user_docs'), ('rust.yml', 'fast')):
      with self.subTest(workflow=name, job=job_id):
        self.assertEqual(workflow(name)['jobs'][job_id]['runs-on'], 'ubuntu-24.04')

  def test_every_rust_execution_job_uses_the_trusted_common_pool(self):
    jobs = workflow('rust.yml')['jobs']
    execution = {job_id: job for job_id, job in jobs.items() if job_id != 'fast'}
    self.assertTrue(execution)
    trusted = "(github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)"
    for job_id, job in execution.items():
      with self.subTest(job=job_id):
        self.assertEqual(job['runs-on'], ['self-hosted', 'Linux', 'X64', 'openpilot-rust'])
        condition = trusted
        if job_id == 'arm64':
          condition += " && (github.event_name != 'push' || github.ref == 'refs/heads/dev' || github.ref == 'refs/heads/main')"
        self.assertEqual(job['if'], '${{ ' + condition + ' }}')
        self.assertNotIn('continue-on-error', job)
        for step in job['steps']:
          self.assertNotIn('continue-on-error', step)
          # Explicit build roots must match the RUNNER_TEMP producer/consumer view.
          for value in step.get('env', {}).values():
            self.assertNotIn('${{ runner.temp }}', value)
    self.assertEqual(jobs['fast']['name'], 'rust checks')
    self.assertEqual(jobs['fast']['if'], '${{ ' + trusted + ' && (always()) }}')

  def test_runtime_foundation_logical_architectures_share_the_self_hosted_pool(self):
    jobs = workflow('runtime-source-build.yml')['jobs']
    jobs.update({'messaging-' + job_id: job
                 for job_id, job in workflow('messaging-allocation-431.yml')['jobs'].items()})
    self.assertTrue(jobs)
    trusted = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) }}"
    for job_id, job in jobs.items():
      with self.subTest(job=job_id):
        self.assertEqual(job['runs-on'], ['self-hosted', 'Linux', 'X64', 'openpilot-rust'])
        self.assertEqual(job['if'], trusted)
        self.assertNotIn('continue-on-error', job)
        self.assertEqual(job['strategy']['matrix']['runner'], ['ubuntu-24.04', 'ubuntu-24.04-arm'])
        self.assertEqual(job['strategy']['fail-fast'], 'false')
        self.assertEqual(job['env']['LOCAL_CI_ARCH'],
                         "${{ matrix.runner == 'ubuntu-24.04-arm' && 'aarch64' || 'x86_64' }}")
        self.assertTrue(any(step.get('uses') == './.github/actions/local-arm'
                            and step.get('with', {}).get('cleanup') != 'true'
                            for step in job['steps']))
        for step in job['steps']:
          self.assertNotIn('continue-on-error', step)

  def test_cross_apt_retains_sources_without_sharing_metadata_locks(self):
    steps = workflow('rust.yml')['jobs']['arm64']['steps']
    setup = next(step['run'] for step in steps
                 if step.get('name') == 'Stage native GNU aarch64 startup renderer and portable loader headers')
    for required in ('startup-ui-cross.sources', 'Dir::Etc::sourceparts=-',
                     'Dir::State::lists="$RUNNER_TEMP/startup-ui-cross-apt/lists"',
                     'Dir::Cache::pkgcache=', 'Dir::Cache::srcpkgcache=',
                     'sudo apt-get "${cross_apt[@]}" update',
                     'sudo apt-get "${cross_apt[@]}" install -y libegl-dev:arm64 libgles-dev:arm64 libdrm-dev:arm64 libgbm-dev:arm64'):
      self.assertIn(required, setup)


  def test_android_builds_prepare_job_owned_sdks_before_setup(self):
    for name, job_id in (('naver-apk-tools.yml', 'host-toolkit'),
                         ('naver-apk-tools.yml', 'payload'), ('jetlink.yml', 'android-core')):
      with self.subTest(workflow=name, job=job_id):
        steps = workflow(name)['jobs'][job_id]['steps']
        setup = next(i for i, step in enumerate(steps)
                     if step.get('uses', '').startswith('android-actions/setup-android@'))
        self.assertEqual(steps[setup - 1]['name'], 'Admit the job-owned Android SDK')
        preparation = steps[setup - 1]['run']
        for env_name in ('CI_JOB_TEMP', 'ANDROID_HOME', 'ANDROID_SDK_ROOT'):
          self.assertIn(env_name, preparation)


if __name__ == '__main__':
  unittest.main()

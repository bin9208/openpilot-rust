"""Regression guard for required-check routing and fail-closed aggregation."""
import ast
import os
import shlex
import subprocess
import tempfile
from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


def workflow(name):
  return yaml.load((ROOT / '.github/workflows' / name).read_text(), Loader=yaml.BaseLoader)


class WorkflowPolicyTests(unittest.TestCase):
  def test_fast_checks_keep_protected_prs_without_parallel_push_runs(self):
    events = workflow('fast-checks.yml')['on']
    self.assertNotIn('push', events)
    self.assertEqual(set(events['pull_request']['branches']), {'dev', 'main'})
    for event in ('pull_request',):
      self.assertNotIn('paths', events[event])
      self.assertNotIn('paths-ignore', events[event])

  def test_integration_runs_before_and_after_merge_without_path_skips(self):
    spec = workflow('integration.yml')
    for event in ('push', 'pull_request'):
      self.assertEqual(spec['on'][event], {'branches': ['dev', 'main']})
    self.assertEqual(spec['jobs']['build']['with']['run_number'], '1')
    gate = spec['jobs']['gate']
    self.assertEqual(set(gate['needs']), {'build', 'navigation', 'apk', 'jetlink'})
    self.assertEqual(gate['if'], "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always()) }}")
    self.assertEqual(len(gate['steps']), 2)
    self.assertEqual(gate['steps'][0]['run'], 'test "$GITHUB_ACTIONS" = true\nif [[ $RUNNER_ENVIRONMENT == self-hosted ]]; then\n  [[ $RUNNER_NAME == bin-actions || $RUNNER_NAME == bin-actions2 || $RUNNER_NAME == bin-actions3 ]]\nelse\n  [[ $RUNNER_ENVIRONMENT == github-hosted && $RUNNER_OS == Linux &&\n     ( $RUNNER_ARCH == X64 || $RUNNER_ARCH == ARM64 ) ]]\nfi\n')
    step = gate['steps'][1]
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

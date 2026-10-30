"""Regression guard for required-check routing and fail-closed aggregation."""
from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
TRUST = ("github.repository == 'bin9208/openpilot-rust' && "
         "(github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)")
TRUST_IF = '${{ ' + TRUST + ' }}'


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
    self.assertEqual(gate['if'], '${{ ' + TRUST + ' && (always()) }}')
    step = gate['steps'][0]
    for name in ('BUILD', 'NAVIGATION', 'APK', 'JETLINK'):
      self.assertIn(f'test "${name}" = success', step['run'])
    for job in ('build', 'navigation', 'apk', 'jetlink'):
      self.assertEqual(spec['jobs'][job]['if'], TRUST_IF)
      self.assertNotIn('continue-on-error', spec['jobs'][job])
      called = Path(spec['jobs'][job]['uses']).name
      self.assertIn('workflow_call', workflow(called)['on'])

  def test_documentation_check_runs_on_protected_branch_pushes(self):
    events = workflow('user-docs.yaml')['on']
    self.assertTrue({'main', 'dev'}.issubset(events['push']['branches']))
    self.assertIn('pull_request', events)


if __name__ == '__main__':
  unittest.main()

"""Regression guard for required-check routing and fail-closed aggregation."""
from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


def workflow(name):
  return yaml.load((ROOT / '.github/workflows' / name).read_text(), Loader=yaml.BaseLoader)


class WorkflowPolicyTests(unittest.TestCase):
  def test_fast_checks_cover_all_branch_pushes_and_protected_prs(self):
    events = workflow('fast-checks.yml')['on']
    self.assertEqual(events['push']['branches'], ['**'])
    self.assertEqual(set(events['pull_request']['branches']), {'dev', 'main'})
    for event in ('push', 'pull_request'):
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

  def test_original_event_receipts_are_limited_to_the_four_actual_validation_jobs(self):
    expected = {('rust.yml', 'workspace'), ('tests.yaml', 'build_release'),
                ('fast-checks.yml', 'fast'), ('user-docs.yaml', 'check_user_docs')}
    found = set()
    for name in ('rust.yml', 'tests.yaml', 'fast-checks.yml', 'user-docs.yaml'):
      for key, job in workflow(name)['jobs'].items():
        steps = [step for step in job.get('steps', []) if step.get('uses') == './.github/actions/job-temp']
        starts = [step for step in steps if step.get('with', {}).get('cleanup') != 'true']
        ends = [step for step in steps if step.get('with', {}).get('cleanup') == 'true']
        if any(step.get('with', {}).get('validation-context') == 'true' for step in starts):
          found.add((name, key))
          self.assertEqual(len(starts), 1)
          self.assertEqual(len(ends), 1)
          self.assertEqual(ends[0]['with']['validation-context'], 'true')
          self.assertEqual(ends[0]['if'], 'always()')
    self.assertEqual(found, expected)
    action = yaml.load((ROOT/'.github/actions/job-temp/action.yml').read_text(), Loader=yaml.BaseLoader)
    uploads = [step for step in action['runs']['steps'] if step.get('uses', '').startswith('actions/upload-artifact@')]
    self.assertEqual(len(uploads), 1)
    self.assertIn("steps.verify_validation_context.outcome == 'success'", uploads[0]['if'])


if __name__ == '__main__':
  unittest.main()

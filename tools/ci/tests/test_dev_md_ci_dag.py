"""Cloud regression: change scheduling, retain every original job and command."""
import copy
from pathlib import Path
import subprocess
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = '96c69111e53fbeaa49d629c634830353bb823cf7'
GROWTH_REPLACEMENTS = (
    ('(25 + 2) * 1024 * 1024 * 1024', '2 * 1024 * 1024 * 1024'),
    ('(25 + 4) * 1024 * 1024 * 1024', '4 * 1024 * 1024 * 1024'),
    ('Retain 25 GiB plus build growth; recover at least 35 GiB before retry.',
     'Insufficient capacity for estimated documentation build growth (2 GiB).'),
    ('Retain 25 GiB plus Android SDK/build growth; recover at least 35 GiB before retry.',
     'Insufficient capacity for estimated Android SDK/build growth (4 GiB).'),
)


def load(name, before=False):
  name = '.github/workflows/'+name
  text = subprocess.check_output(['git', 'show', BASE+':'+name], cwd=ROOT, text=True) if before else (ROOT/name).read_text()
  return yaml.load(text, Loader=yaml.BaseLoader)


class CiDagTests(unittest.TestCase):
  def test_every_original_command_condition_runner_and_artifact_remains(self):
    before, after = load('ci.yml', True), load('ci.yml')
    growth_updates = {}
    for job in before['jobs'].values():
      for step in job['steps']:
        original = updated = step.get('run', '')
        for old, new in GROWTH_REPLACEMENTS:
          updated = updated.replace(old, new)
        if updated != original: growth_updates[updated] = original
    self.assertEqual(len(growth_updates), 3)
    self.assertEqual(set(before['jobs']), set(after['jobs']))
    for name, original in before['jobs'].items():
      with self.subTest(job=name):
        current = copy.deepcopy(after['jobs'][name])
        if name not in {'check_user_docs', 'integration-gate', 'fast', 'arm64', 'runtime-delivery'}:
          self.assertEqual(current['needs'], ['check_user_docs'])
          current['needs'] = original['needs']
          if 'max-parallel' in original.get('strategy', {}):
            self.assertEqual(original['strategy']['max-parallel'], '1')
            self.assertNotIn('max-parallel', current['strategy'])
            current['strategy']['max-parallel'] = '1'
        if name == 'check_user_docs':
          current['steps'] = [step for step in current['steps'] if not (
              step.get('uses') == 'actions/setup-python@v6'
              or step.get('name') == 'Verify the complete CI graph and unchanged delivery boundary')]
        if name == 'xiaoge-runtime':
          # The original shallow checkout omitted its frozen Rust oracle SHA.
          # Fetch only that independent source; preserve the byte/hash checks.
          fetch = dict(name='Fetch exact frozen Rust Xiaoge oracle source',
              run='git fetch --no-tags --depth=1 origin 0682e933d3fea06e8953ace63fc521a29ee02b6e')
          self.assertEqual(current['steps'].count(fetch), 1)
          self.assertLess(current['steps'].index(fetch), next(i for i, step in enumerate(current['steps'])
              if step.get('name') == 'Fetch exact upstream reference commits'))
          current['steps'].remove(fetch)
        for step in current['steps']:
          if step.get('run') in growth_updates:
            step['run'] = growth_updates[step['run']]
        self.assertEqual(current, original)
    self.assertEqual(after['on'], before['on'])
    self.assertEqual(after['permissions'], before['permissions'])
    self.assertEqual(after['concurrency']['cancel-in-progress'], 'false')
    self.assertIn('github.sha', after['concurrency']['group'])

  def test_required_four_gates_and_delivery_cover_every_job(self):
    document = load('ci.yml')
    jobs = document['jobs']
    def dependencies(name):
      needs = jobs[name].get('needs', [])
      return [needs] if isinstance(needs, str) else needs
    def ancestors(name, seen=None):
      seen = set() if seen is None else seen
      for parent in dependencies(name):
        self.assertNotIn(parent, seen, 'cycle in CI graph')
        yield parent
        yield from ancestors(parent, seen | {name})
    self.assertEqual(set(ancestors('runtime-delivery')), set(jobs)-{'runtime-delivery'})
    self.assertEqual(dependencies('arm-build'), ['check_user_docs'])
    for name in ('git_update', 'build_release', 'workspace', 'model-memory', 'arm-build'):
      self.assertEqual(dependencies(name), ['check_user_docs'])
    # No artifact or job output dependency may silently be removed.
    for job in jobs.values():
      self.assertNotIn('outputs', job)
      for step in job['steps']:
        self.assertNotIn('download-artifact', step.get('uses', ''))
        self.assertNotIn('needs.', str(step))
    for key, expected in [('integration-gate', 'integration gate'), ('fast', 'rust checks'),
                          ('arm64', 'rust aarch64 build'), ('check_user_docs', 'check mapped user docs')]:
      self.assertIn(expected, jobs[key]['name'])
    delivery = jobs['runtime-delivery']
    self.assertIn("github.event_name == 'push'", delivery['if'])
    self.assertIn("github.ref == 'refs/heads/dev-md'", delivery['if'])
    self.assertIn('[runtime-package]', delivery['if'])
    self.assertEqual(delivery, load('ci.yml', True)['jobs']['runtime-delivery'])

  def test_package_changes_only_retired_fast_requirement_and_fixed_disk_floor(self):
    before, after = load('dev-md-runtime.yml', True), load('dev-md-runtime.yml')
    old = '{"fast checks","integration gate","check mapped user docs","rust checks","rust aarch64 build"}'
    new = '{"integration gate","check mapped user docs","rust checks","rust aarch64 build"}'
    expected = copy.deepcopy(before)
    replacements = 0
    for job in expected['jobs'].values():
      for step in job.get('steps', []):
        if old in step.get('run', ''):
          step['run'] = step['run'].replace(old, new)
          replacements += 1
        if 'run' in step:
          step['run'] = step['run'].replace('(25+4)*2**30', '4*2**30').replace(
              '(25+2)*2**30', '2*2**30').replace('disk reserve before dependency installation',
              'insufficient capacity for estimated dependency installation growth')
    self.assertEqual(replacements, 1)
    self.assertEqual(after, expected)
    self.assertEqual(set(after['on']), {'workflow_dispatch'})


if __name__ == '__main__': unittest.main()

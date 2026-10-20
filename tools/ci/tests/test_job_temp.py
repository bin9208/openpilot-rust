"""Persistent runners need isolated, consistently collected and bounded output."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
ACTION = yaml.load((ROOT / '.github/actions/job-temp/action.yml').read_text(), Loader=yaml.BaseLoader)


def invoke(script, environment):
    return subprocess.run(['bash', '-eu', '-c', script], env=environment, capture_output=True, text=True)


class JobTempTests(unittest.TestCase):
    def test_two_jobs_get_fresh_tool_and_artifact_paths_and_cleanup_only_their_output(self):
        with tempfile.TemporaryDirectory(prefix='runner temp ') as temporary:
            root = Path(temporary)
            stale = root / 'torque-numerics'
            stale.mkdir()
            (stale / 'receipt').write_text('retain')
            outputs = []
            for attempt in range(2):
                github_env = root / f'github-env-{attempt}'
                environment = dict(os.environ, RUNNER_TEMP=str(root), GITHUB_ENV=str(github_env),
                                   GITHUB_RUN_ID='42', GITHUB_RUN_ATTEMPT='1', GITHUB_JOB='workspace')
                environment.pop('BASH_ENV', None)
                result = invoke(ACTION['runs']['steps'][0]['run'], environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                environment.update(dict(line.split('=', 1) for line in github_env.read_text().splitlines()))
                owned = Path(environment['CI_JOB_TEMP'])
                outputs.append(owned)
                result = invoke("python3 -c 'import json,os;print(json.dumps([os.environ.get(\"RUNNER_TEMP\"),os.environ.get(\"TMPDIR\")]))'", environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), [str(owned), str(owned)])
                # A tool's output matches the path passed to artifact/cache actions.
                (owned / 'report.json').write_text('evidence')
                self.assertTrue((Path(environment['CI_JOB_TEMP']) / 'report.json').is_file())
                # Explicit test overrides and unset fallback cases must not be
                # replaced when their nested bash process reads BASH_ENV.
                fixture = dict(environment, RUNNER_TEMP=str(root / 'fixture'))
                result = invoke('printf "%s" "${RUNNER_TEMP-unset}"', fixture)
                self.assertEqual(result.stdout, fixture['RUNNER_TEMP'])
                fixture.pop('RUNNER_TEMP')
                result = invoke('printf "%s" "${RUNNER_TEMP-unset}"', fixture)
                self.assertEqual(result.stdout, 'unset')
                result = invoke(ACTION['runs']['steps'][1]['run'], environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(owned.exists())
                self.assertEqual((stale / 'receipt').read_text(), 'retain')
            self.assertNotEqual(outputs[0], outputs[1])

    def test_cleanup_rejects_unrelated_nested_and_symlink_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            outside = root / 'unrelated'
            outside.mkdir()
            (outside / 'keep').write_text('keep')
            link = root / 'openpilot-ci-link'
            link.symlink_to(outside, target_is_directory=True)
            for path in (outside, root / 'openpilot-ci-fake/../unrelated', link):
                environment = dict(os.environ, CI_RUNNER_TEMP=str(root), CI_JOB_TEMP=str(path))
                environment.pop('BASH_ENV', None)
                result = invoke(ACTION['runs']['steps'][1]['run'], environment)
                self.assertNotEqual(result.returncode, 0, str(path))
                self.assertEqual((outside / 'keep').read_text(), 'keep')

    def test_all_concrete_jobs_initialize_and_collect_the_same_output_before_cleanup(self):
        for name in ('ci.yml', 'dev-md-runtime.yml'):
            text = (ROOT / '.github/workflows' / name).read_text()
            self.assertNotIn('${{ runner.temp }}', text)
            jobs = yaml.load(text, Loader=yaml.BaseLoader)['jobs']
            for key, job in jobs.items():
                steps = job.get('steps', [])
                checkout = [i for i, step in enumerate(steps) if step.get('uses', '').startswith('actions/checkout@')]
                if not checkout:
                    continue
                with self.subTest(workflow=name, job=key):
                    init = steps[checkout[0] + 1]
                    self.assertEqual(init['uses'], './.github/actions/job-temp')
                    self.assertNotIn('cleanup', init.get('with', {}))
                    self.assertEqual(steps[-1]['uses'], './.github/actions/job-temp')
                    self.assertEqual(steps[-1]['if'], 'always()')
                    self.assertEqual(steps[-1]['with']['cleanup'], 'true')


if __name__ == '__main__':
    unittest.main()

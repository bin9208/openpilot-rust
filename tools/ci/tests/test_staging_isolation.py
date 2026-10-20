"""Retries must stage fresh pinned providers without trusting prior temp output."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


class StagingIsolationTests(unittest.TestCase):
    def test_every_raylib_stage_uses_a_fresh_output_and_exports_it(self):
        jobs = yaml.load((ROOT / '.github/workflows/ci.yml').read_text(), Loader=yaml.BaseLoader)['jobs']
        stages = [(name, step['run']) for name, job in jobs.items() for step in job.get('steps', [])
                  if '--bin stage-startup-ui-raylib --' in step.get('run', '')]
        self.assertEqual(len(stages), 5)
        for name, script in stages:
            with self.subTest(job=name), tempfile.TemporaryDirectory(prefix='staging isolation ') as temporary:
                root = Path(temporary)
                guard = root / 'workspace/rust/tools/check-disk-space'
                guard.parent.mkdir(parents=True)
                guard.write_text('#!/bin/sh\nexit 0\n')
                guard.chmod(0o755)
                commands = root / 'commands'
                commands.mkdir()
                cargo = commands / 'cargo'
                cargo.write_text('''#!/usr/bin/env python3
import os,sys
from pathlib import Path
output=Path(sys.argv[sys.argv.index('--output')+1])
assert not output.exists(), 'stale output was reused'
output.mkdir()
(output/'fixture').write_text('staged')
with open(os.environ['STAGED_PATHS'],'a') as file:file.write(str(output)+'\\n')
''')
                cargo.chmod(0o755)
                prior = root / 'startup-raylib-host'
                prior.mkdir()
                (prior / 'SOURCE_SHA256').write_text('invalid previous generation')
                environment = dict(os.environ, PATH=str(commands) + ':' + os.environ['PATH'],
                                   RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root / 'workspace'),
                                   GITHUB_RUN_ID='42', GITHUB_RUN_ATTEMPT='1',
                                   GITHUB_ENV=str(root / 'environment'), STAGED_PATHS=str(root / 'paths'))
                # Run the actual preparation/invocation/export statements with owned
                # tool substitutes; package installs and real builds stay in Actions.
                selected = '\n'.join(line for line in script.splitlines()
                                     if ('raylib_root=' in line or 'check-disk-space' in line
                                         or '--bin stage-startup-ui-raylib --' in line
                                         or line.lstrip().startswith("printf 'STARTUP_UI_RAYLIB_ROOT=")))
                for _ in range(2):
                    result = subprocess.run(['bash', '-eu', '-c', selected], env=environment,
                                            capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                paths = (root / 'paths').read_text().splitlines()
                self.assertEqual(len(paths), 2)
                self.assertNotEqual(paths[0], paths[1])
                exported = (root / 'environment').read_text()
                for path in paths:
                    self.assertIn('STARTUP_UI_RAYLIB_ROOT=' + path + '\n', exported)
                    self.assertTrue((Path(path) / 'fixture').is_file())
                self.assertEqual((prior / 'SOURCE_SHA256').read_text(), 'invalid previous generation')


if __name__ == '__main__':
    unittest.main()

"""Real Git source cleanliness with the existing job-owned Cargo target view."""
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


def git(source, *args):
    return subprocess.check_output(['/usr/bin/git', '-C', str(source), '-c', 'user.name=Owned CI fixture',
        '-c', 'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', *args])


def checkout(base, patterns):
    source = base/'source'
    (source/'rust').mkdir(parents=True)
    (source/'rust/.gitignore').write_text(patterns)
    (source/'rust/Cargo.toml').write_text('[workspace]\n')
    (source/'tools/ci').mkdir(parents=True)
    for name in ('native-apt-shell', 'runner_policy.py'):
        (source/'tools/ci'/name).write_bytes((ROOT/'tools/ci'/name).read_bytes())
    git(source, 'init', '--quiet')
    git(source, 'add', '.')
    git(source, 'commit', '--quiet', '-m', 'Owned source fixture')
    return source


class CargoTargetViewTests(unittest.TestCase):
    def test_directory_only_ignore_reproduces_the_dirty_source_and_exact_path_rule_fixes_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            patterns = (ROOT/'rust/.gitignore').read_text()
            previous = patterns.replace('/target\n', '/target/\n')
            self.assertNotEqual(previous, patterns)
            source = checkout(base, previous)
            cache = base/'owned-cargo-output'
            cache.mkdir()
            (source/'rust/target').symlink_to(cache, target_is_directory=True)
            self.assertIn(b'?? rust/target\0', git(source, 'status', '--porcelain', '-z', '--untracked-files=all'))
            (source/'rust/.gitignore').write_text(patterns)
            git(source, 'add', 'rust/.gitignore')
            git(source, 'commit', '--quiet', '-m', 'Ignore the generated target path')
            self.assertEqual(git(source, 'status', '--porcelain', '-z', '--untracked-files=all'), b'')
            (source/'rust/Cargo.toml').write_text('[workspace]\n# actual source modification\n')
            self.assertIn(b' M rust/Cargo.toml\0', git(source, 'status', '--porcelain', '-z'))
            (source/'rust/unexpected-target').symlink_to(cache, target_is_directory=True)
            self.assertIn(b'?? rust/unexpected-target\0', git(source, 'status', '--porcelain', '-z', '--untracked-files=all'))

    def test_actual_job_helper_leaves_source_clean_and_removes_only_its_own_view(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            source = checkout(base, (ROOT/'rust/.gitignore').read_text())
            runner_temp = base/'runner-temp'
            runner_temp.mkdir()
            environment_file = base/'github-env'
            action = yaml.load((ROOT/'.github/actions/job-temp/action.yml').read_text(), Loader=yaml.BaseLoader)
            scripts = [step['run'] for step in action['runs']['steps']]
            environment = dict(PATH='/usr/bin:/bin', HOME=str(base), GITHUB_ACTIONS='true',
                RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='bin-actions3', LOCAL_CI_ARCH=platform.machine(),
                RUNNER_TEMP=str(runner_temp), GITHUB_WORKSPACE=str(source), GITHUB_ENV=str(environment_file),
                GITHUB_ACTION_PATH=str(ROOT/'.github/actions/job-temp'),
                GITHUB_RUN_ID='12345', GITHUB_RUN_ATTEMPT='1', GITHUB_JOB='owned-target-view')
            values = {}
            try:
                subprocess.run(['/bin/bash', '-e', '-c', scripts[0]], env=environment, check=True)
                values = dict(line.split('=', 1) for line in environment_file.read_text().splitlines())
                target = source/'rust/target'
                self.assertTrue(target.is_symlink())
                self.assertEqual(os.readlink(target), str(Path(values['CI_JOB_TEMP'])/'cargo-target'))
                owned = Path(values['CI_JOB_TEMP'])
                fixture_temp = base/'fixture-temp'
                for variable, suffix in (('CARGO_TARGET_DIR', 'logger-asan-target'),
                                         ('FFMPEG_DIR', 'logger-ffmpeg-gnu/install')):
                    derived = runner_temp/suffix
                    external = base/'external'/suffix
                    cases = (
                        ('derived', runner_temp, derived, owned, owned/suffix),
                        ('external', runner_temp, external, owned, external),
                        ('guest-source', runner_temp, Path('/native/state')/suffix,
                         owned, Path('/native/state')/suffix),
                        ('sibling-prefix', runner_temp, Path(str(runner_temp)+'-other')/suffix,
                         owned, Path(str(runner_temp)+'-other')/suffix),
                        ('runner-override', fixture_temp, derived, fixture_temp, derived),
                    )
                    for case, initial_runner, initial_path, expected_runner, expected_path in cases:
                        with self.subTest(variable=variable, case=case):
                            shell_environment = {**environment, **values,
                                'RUNNER_TEMP': str(initial_runner), 'TMPDIR': str(fixture_temp),
                                variable: str(initial_path)}
                            shell_env = shell_environment.pop('BASH_ENV')
                            actual = subprocess.check_output(['/bin/bash', '-e', '-c',
                                'source "$1"\nprintf "%s\\n" "$RUNNER_TEMP" "$TMPDIR" "${'+variable+'-}"',
                                'job-temp-fixture', shell_env], env=shell_environment, text=True).splitlines()
                            expected_tmp = owned if initial_runner == runner_temp else fixture_temp
                            self.assertEqual(actual, [str(expected_runner), str(expected_tmp), str(expected_path)])
                (target/'owned-generated-artifact').write_bytes(b'owned build output')
                self.assertEqual(git(source, 'status', '--porcelain', '-z', '--untracked-files=all'), b'')
                (source/'preserved-source-change').write_text('owned source fixture\n')
                subprocess.run(['/bin/bash', '-e', '-c', scripts[1]], env={**environment, **values}, check=True)
                self.assertFalse(target.exists())
                self.assertFalse(target.is_symlink())
                self.assertTrue((source/'preserved-source-change').is_file())
                self.assertIn(b'?? preserved-source-change\0', git(source, 'status', '--porcelain', '-z', '--untracked-files=all'))
            finally:
                if values and Path(values['CI_JOB_TEMP']).exists():
                    subprocess.run(['/bin/bash', '-e', '-c', scripts[1]], env={**environment, **values}, check=True)


if __name__ == '__main__':
    unittest.main()

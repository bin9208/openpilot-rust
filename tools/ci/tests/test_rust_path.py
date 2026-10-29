"""Provisioned Rust must remain visible after language setup actions."""
import ast
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import yaml

ROOT = Path(__file__).resolve().parents[3]


def action_script():
  spec = yaml.load((ROOT / '.github/actions/rust-path/action.yml').read_text(), Loader=yaml.BaseLoader)
  return spec['runs']['steps'][-1]['run']


class RustPathTests(unittest.TestCase):
  def test_declared_cargo_home_and_existing_custom_path(self):
    for existing in (False, True):
      with self.subTest(existing=existing), tempfile.TemporaryDirectory(prefix='rust path ') as temporary:
        root = Path(temporary)
        binary = root / ('custom bin' if existing else 'cargo home/bin')
        binary.mkdir(parents=True)
        for name in ('cargo', 'rustup'):
          path = binary / name
          path.write_text('#!/bin/sh\nexit 99\n')
          path.chmod(0o755)
        output = root / 'github-path'
        env = dict(os.environ, CARGO_HOME=str(root / 'cargo home'), GITHUB_PATH=str(output),
                   PATH=(str(binary) + ':' if existing else '') + '/usr/bin:/bin')
        result = subprocess.run(['bash', '--noprofile', '--norc', '-eu', '-c', action_script()],
                                env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output.read_text(), str(binary) + '\n')
        # Neither shim is executed; this helper only registers the existing tools.

  def test_missing_tools_fail_before_any_path_is_published(self):
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      output = root / 'github-path'
      env = dict(os.environ, CARGO_HOME=str(root / 'absent'), GITHUB_PATH=str(output), PATH='/usr/bin:/bin')
      result = subprocess.run(['bash', '--noprofile', '--norc', '-eu', '-c', action_script()],
                              env=env, capture_output=True, text=True)
      self.assertNotEqual(result.returncode, 0)
      self.assertIn('Rust tools are not provisioned', result.stderr)
      self.assertFalse(output.exists())

  def test_bootstrap_refuses_low_space_and_unverified_downloads(self):
    spec = yaml.load((ROOT / '.github/actions/rust-path/action.yml').read_text(), Loader=yaml.BaseLoader)
    script = spec['runs']['steps'][1]['run']
    for low_space in (True, False):
      with self.subTest(low_space=low_space), tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        commands = root / 'commands'
        commands.mkdir()
        calls = root / 'curl-calls'
        df = commands / 'df'
        df.write_text('#!/bin/sh\nprintf "Avail\\n%s\\n" ' + ('1024' if low_space else '999999999999') + '\n')
        df.chmod(0o755)
        curl = commands / 'curl'
        curl.write_text('''#!/bin/sh
printf 'called\\n' >> "$CURL_CALLS"
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then shift; printf '#!/bin/sh\\nexit 99\\n' > "$1"; break; fi
  shift
done
''')
        curl.chmod(0o755)
        env = dict(os.environ, PATH=str(commands) + ':/usr/bin:/bin', RUNNER_TEMP=str(root), CURL_CALLS=str(calls))
        result = subprocess.run(['bash', '--noprofile', '--norc', '-eu', '-o', 'pipefail', '-c', script],
                                env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        if low_space:
          self.assertIn('estimated Rust bootstrap growth', result.stderr)
          self.assertFalse(calls.exists())
        else:
          self.assertTrue(calls.exists())
          self.assertIn('FAILED', result.stdout)
        self.assertFalse(list(root.glob('rustup-bootstrap.*')))

  def test_persistent_runner_cache_never_cleans_or_restores_tool_binaries(self):
    spec = yaml.load((ROOT / '.github/workflows/ci.yml').read_text(), Loader=yaml.BaseLoader)
    for job in spec['jobs'].values():
      for step in job.get('steps', []):
        if step.get('uses', '').startswith('Swatinem/rust-cache@'):
          self.assertEqual(step['with']['cache-bin'], 'false')
          self.assertEqual(step['with']['prefix-key'], 'v1-rust-no-bins')

  def test_available_cpu_defaults_and_explicit_overrides_reach_later_steps(self):
    spec = yaml.load((ROOT / '.github/actions/rust-path/action.yml').read_text(), Loader=yaml.BaseLoader)
    script = next(step['run'] for step in spec['runs']['steps']
                  if step.get('name', '').startswith('Select available CPU parallelism'))
    count = len(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else os.cpu_count()
    for overrides in ({}, {'CARGO_BUILD_JOBS': '13'}, {'RUST_TEST_THREADS': '7'},
                      {'CARGO_BUILD_JOBS': '13', 'RUST_TEST_THREADS': '7'}):
      with self.subTest(overrides=overrides), tempfile.TemporaryDirectory() as temporary:
        output = Path(temporary)/'github-env'
        environment = dict(os.environ, GITHUB_ENV=str(output))
        environment.pop('CARGO_BUILD_JOBS', None)
        environment.pop('RUST_TEST_THREADS', None)
        environment.update(overrides)
        result = subprocess.run(['bash', '-eu', '-o', 'pipefail', '-c', script],
                                env=environment, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        saved = dict(line.split('=', 1) for line in output.read_text().splitlines()) if output.exists() else {}
        self.assertEqual(saved, {key: str(count) for key in ('CARGO_BUILD_JOBS', 'RUST_TEST_THREADS')
                                 if key not in overrides})
        self.assertTrue(all(key not in saved for key in overrides))

  def test_all_explicit_rust_consumers_register_path_before_setup(self):
    for filename in ('ci.yml', 'dev-md-runtime.yml'):
      jobs = yaml.load((ROOT / '.github/workflows' / filename).read_text(), Loader=yaml.BaseLoader)['jobs']
      for key, job in jobs.items():
        steps = job.get('steps', [])
        commands = '\n'.join(step.get('run', '') for step in steps)
        if not any(token in commands for token in ('cargo ', 'rustup ', 'package.py host-tools', 'package.py publish', 'scons', './tools/op.sh setup')):
          continue
        with self.subTest(workflow=filename, job=key):
          checkout = next(i for i, step in enumerate(steps) if step.get('uses', '').startswith('actions/checkout@'))
          helper = next(i for i, step in enumerate(steps) if step.get('uses') == './.github/actions/rust-path')
          self.assertEqual(steps[checkout + 1]["uses"], "./.github/actions/job-temp")
          self.assertEqual(helper, checkout + 2)
          for i, step in enumerate(steps):
            if step.get('uses', '').startswith('actions/setup-python@'):
              self.assertLess(helper, i)


class CargoConcurrencyTests(unittest.TestCase):
    def environment_assignment(self, filename, function, variable, inherited):
        path = ROOT/filename
        tree = ast.parse(path.read_text())
        function = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == function)
        assignment = next(n for n in ast.walk(function) if isinstance(n, ast.Assign)
                          and any(isinstance(t, ast.Name) and t.id == variable for t in n.targets))
        source = compile(ast.Module(body=[assignment], type_ignores=[]), str(path), 'exec')
        namespace = dict(os=os, TARGET=Path('/owned/target'), flags='fixture-sanitizer-flags',
                         library=Path('/owned/asan.so'),
                         args=SimpleNamespace(target_dir=Path('/owned/target'), opencv=Path('/owned/opencv')))
        # Only the actual environment dictionary is evaluated: no build, external
        # command, filesystem initialization or other helper statement is executed.
        with patch.dict(os.environ, inherited, clear=True):
            exec(source, namespace)
        return namespace[variable]

    def environments(self, inherited):
        for filename, function, variable in (
            ('rust/ci/runtime/package.py', 'environment', 'env'),
            ('rust/tools/check_xiaoge_native_memory.py', 'main', 'environment'),
        ):
            yield filename, self.environment_assignment(filename, function, variable, inherited)

    def test_absent_override_reaches_cargo_and_libtest_without_a_hidden_cap(self):
        for filename, environment in self.environments({}):
            with self.subTest(filename=filename):
                self.assertNotIn('CARGO_BUILD_JOBS', environment)
                self.assertNotIn('RUST_TEST_THREADS', environment)

    def test_explicit_user_build_and_test_parallelism_survives_child_environment(self):
        overrides = dict(CARGO_BUILD_JOBS='13', RUST_TEST_THREADS='7')
        for filename, environment in self.environments(overrides):
            with self.subTest(filename=filename):
                for key, value in overrides.items():
                    self.assertEqual(environment[key], value)

if __name__ == '__main__':
  unittest.main()

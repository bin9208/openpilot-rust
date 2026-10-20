"""Provisioned Rust must remain visible after language setup actions."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


def action_script():
  spec = yaml.load((ROOT / '.github/actions/rust-path/action.yml').read_text(), Loader=yaml.BaseLoader)
  return spec['runs']['steps'][0]['run']


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

  def test_all_explicit_rust_consumers_register_path_before_setup(self):
    for filename in ('ci.yml', 'dev-md-runtime.yml'):
      jobs = yaml.load((ROOT / '.github/workflows' / filename).read_text(), Loader=yaml.BaseLoader)['jobs']
      for key, job in jobs.items():
        steps = job.get('steps', [])
        commands = '\n'.join(step.get('run', '') for step in steps)
        if not any(token in commands for token in ('cargo ', 'rustup ', 'package.py host-tools')):
          continue
        with self.subTest(workflow=filename, job=key):
          checkout = next(i for i, step in enumerate(steps) if step.get('uses', '').startswith('actions/checkout@'))
          helper = next(i for i, step in enumerate(steps) if step.get('uses') == './.github/actions/rust-path')
          self.assertEqual(helper, checkout + 1)
          for i, step in enumerate(steps):
            if step.get('uses', '').startswith('actions/setup-python@'):
              self.assertLess(helper, i)


if __name__ == '__main__':
  unittest.main()

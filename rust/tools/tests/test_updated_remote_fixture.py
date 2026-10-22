"""The updater's transport fixture must preserve actual Git URL/ref behavior."""
from pathlib import Path
import subprocess

from updated_fixtures import REMOTE_URL, git, seed, setup


def test_supported_configured_url_maps_only_during_real_git_transport(tmp_path):
  remote, previous = seed(tmp_path)
  config, env = setup(tmp_path / 'runtime', remote, previous, Path('/unused-launcher'))
  base = Path(config['paths']['base'])

  def run(*args):
    return subprocess.check_output(['git', *args], cwd=base, env=env, text=True).strip()

  assert run('remote', 'get-url', 'origin') == REMOTE_URL
  assert run('config', '--get', 'remote.origin.url') == REMOTE_URL
  assert run('ls-remote', 'origin', 'refs/heads/dev').split()[0] == git(remote, 'rev-parse', 'dev')
  run('fetch', 'origin', 'dev')
  assert run('rev-parse', 'origin/dev') == git(remote, 'rev-parse', 'dev')
  assert run('rev-parse', 'HEAD') == previous
  assert run('remote', 'get-url', 'origin') == REMOTE_URL

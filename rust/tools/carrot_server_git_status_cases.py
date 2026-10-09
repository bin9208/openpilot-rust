from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess


GIT = '/usr/bin/git'


class Fixtures:
  def __init__(self, root: Path):
    self.root = root
    root.mkdir(parents=True)
    self.commands: list[dict] = []
    config = root / 'global.gitconfig'
    config.write_text('[user]\n name = Owned Git Status\n email = fixture@example.invalid\n[init]\n defaultBranch = main\n')
    self.env = dict(os.environ, GIT_CONFIG_GLOBAL=str(config), GIT_CONFIG_NOSYSTEM='1', GIT_TERMINAL_PROMPT='0',
                    GIT_AUTHOR_DATE='2000-01-01T00:00:00Z', GIT_COMMITTER_DATE='2000-01-01T00:00:00Z')
    self.bare = root / 'origin.git'
    self.git(root, 'init', '--bare', str(self.bare))
    seed = root / 'seed'
    self.git(root, 'clone', str(self.bare), str(seed))
    (seed / 'base').write_text('base\n')
    self.git(seed, 'add', 'base')
    self.git(seed, 'commit', '-m', 'base')
    self.git(seed, 'push', 'origin', 'main')
    self.head = self.git(seed, 'rev-parse', 'HEAD')

  def git(self, repo: Path, *args: str) -> str:
    command = [GIT, *args]
    result = subprocess.run(command, cwd=repo, env=self.env, capture_output=True, text=True, timeout=30)
    self.commands.append(dict(command=command, cwd=str(repo), code=result.returncode, stdout=result.stdout, stderr=result.stderr))
    if result.returncode != 0:
      raise RuntimeError(f'owned fixture command failed: {result.stderr}')
    return result.stdout.strip()

  def clone(self, name: str) -> Path:
    repo = self.root / name
    self.git(self.root, 'clone', str(self.bare), str(repo))
    return repo

  def commit(self, repo: Path, name: str) -> str:
    (repo / name).write_text(name + '\n')
    self.git(repo, 'add', name)
    self.git(repo, 'commit', '-m', name)
    return self.git(repo, 'rev-parse', 'HEAD')

  def cases(self):
    yield 'tracking', self.clone('tracking')
    repo = self.clone('ahead')
    self.commit(repo, 'ahead')
    yield 'ahead', repo
    repo = self.clone('behind')
    self.git(repo, 'checkout', '-b', 'future')
    target = self.commit(repo, 'future')
    self.git(repo, 'push', 'origin', 'future')
    self.git(repo, 'checkout', 'main')
    self.git(repo, 'config', 'branch.main.merge', 'refs/heads/future')
    yield 'behind', repo
    repo = self.clone('diverged')
    self.commit(repo, 'local')
    self.git(repo, 'config', 'branch.main.merge', 'refs/heads/future')
    yield 'diverged', repo
    repo = self.clone('configured_nested')
    self.git(repo, 'remote', 'rename', 'origin', 'team')
    self.git(repo, 'push', 'team', 'HEAD:refs/heads/topic/nested')
    self.git(repo, 'config', 'branch.main.merge', 'refs/heads/topic/nested')
    yield 'configured_nested', repo
    for name in ['duplicate_distinct', 'duplicate_identical']:
      repo = self.clone(name)
      self.git(repo, 'config', '--add', 'branch.main.merge', 'refs/heads/future' if name == 'duplicate_distinct' else 'refs/heads/main')
      yield name, repo
    for name in ['fallback_origin', 'fallback_first']:
      repo = self.clone(name)
      self.git(repo, 'config', '--remove-section', 'branch.main')
      if name == 'fallback_first':
        self.git(repo, 'remote', 'rename', 'origin', 'alpha')
      yield name, repo
    repo = self.clone('upstream_fallback')
    self.git(repo, 'config', 'branch.main.remote', '.')
    self.git(repo, 'config', 'branch.main.merge', 'refs/heads/future')
    self.git(repo, 'branch', 'future', target)
    yield 'upstream_fallback', repo
    for name in ['no_upstream', 'unborn', 'detached', 'fetch_missing', 'no_compare']:
      repo = self.clone(name)
      match name:
        case 'no_upstream': self.git(repo, 'remote', 'remove', 'origin')
        case 'unborn':
          self.git(repo, 'checkout', '--orphan', 'empty')
          self.git(repo, 'rm', '-rf', '.')
        case 'detached': self.git(repo, 'checkout', '--detach', 'HEAD')
        case 'fetch_missing': self.git(repo, 'remote', 'set-url', 'origin', str(self.root / 'missing-owned.git'))
        case 'no_compare': self.git(repo, 'config', 'branch.main.merge', 'refs/heads/absent')
      yield name, repo
    repo = self.root / 'not_repo'
    repo.mkdir()
    yield 'not_repo', repo

  def save(self) -> None:
    (self.root / 'setup-commands.json').write_text(json.dumps(self.commands, indent=2) + '\n')

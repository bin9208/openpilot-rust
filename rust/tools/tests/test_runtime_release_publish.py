from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import shutil

import pytest

from runtime_release_support import REPOSITORY, ReleaseError
from test_runtime_release import TOOLS, git, package_cli
from test_runtime_release import release_source as release_source


GATES = {'rust.yml': ['rust checks', 'rust aarch64 build'], 'fast-checks.yml': ['fast checks'],
         'integration.yml': ['integration gate'], 'user-docs.yaml': ['check mapped user docs']}


class OwnedGithub:
  """An owned release-store fixture replaces only GitHub network I/O."""

  def __init__(self, root: Path, store: Path):
    self.commit = git(root, 'rev-parse', 'HEAD')
    self.store = store
    self.store.mkdir()
    self.release = None
    self.mutations = []
    self.runs = [{'id': index, 'path': f'.github/workflows/{path}', 'head_sha': self.commit, 'head_branch': 'dev',
                  'event': 'push', 'status': 'completed', 'conclusion': 'success', 'repository': {'full_name': REPOSITORY},
                  'html_url': f'https://github.com/{REPOSITORY}/actions/runs/{index}'}
                 for index, path in enumerate(GATES, 1)]
    self.jobs = {index: [{'id': index * 10 + item, 'name': name, 'status': 'completed', 'conclusion': 'success', 'head_sha': self.commit}
                        for item, name in enumerate(names)] for index, names in enumerate(GATES.values(), 1)}
    self.checks = [dict(job, app={'id': 1}) for jobs in self.jobs.values() for job in jobs]
    self.tag_sha = None
    self.full_name = REPOSITORY
    self.bad_download = False
    self.omit_digest = False

  def api(self, endpoint, *, method='GET', body=None, missing=False, paginate=False):
    suffix = endpoint.removeprefix(f'repos/{REPOSITORY}/')
    if method != 'GET':
      self.mutations.append((method, suffix))
      if method == 'POST' and suffix == 'releases':
        self.release = dict(body, id=10, assets=[], html_url=f'https://github.com/{REPOSITORY}/releases/tag/runtime-{self.commit}')
        return self.release
      if method == 'PATCH' and suffix == 'releases/10':
        self.release.update(body)
        self.tag_sha = self.commit
        return self.release
      raise AssertionError((method, suffix))
    if endpoint == f'repos/{REPOSITORY}':
      return {'full_name': self.full_name, 'private': False}
    if suffix == 'branches/dev/protection/required_status_checks':
      return {'strict': True, 'contexts': [job['name'] for job in self.checks], 'checks': []}
    if suffix.startswith('commits/') and 'check-runs' in suffix:
      return [{'total_count': len(self.checks), 'check_runs': self.checks}] if paginate else {'check_runs': self.checks}
    if suffix.startswith('commits/') and '/status' in suffix:
      return [{'state': 'success', 'statuses': []}] if paginate else {'state': 'success', 'statuses': []}
    if suffix.startswith('actions/runs?'):
      return [{'total_count': len(self.runs), 'workflow_runs': self.runs}]
    if suffix.startswith('actions/runs/') and '/jobs' in suffix:
      return [{'total_count': len(self.jobs[int(suffix.split('/')[2])]), 'jobs': self.jobs[int(suffix.split('/')[2])]}]
    if suffix.startswith('git/ref/tags/'):
      return None if self.tag_sha is None else {'object': {'type': 'commit', 'sha': self.tag_sha}}
    if suffix.startswith('releases?'):
      return [[self.release] if self.release is not None else []]
    if suffix == 'releases/10':
      return self.release
    if suffix.startswith('releases/10/assets'):
      return [self.release['assets']]
    raise AssertionError(endpoint)

  def upload(self, tag: str, path: Path) -> None:
    self.mutations.append(('UPLOAD', path.name))
    shutil.copyfile(path, self.store / path.name)
    self.release['assets'].append({'id': len(self.release['assets']) + 100, 'name': path.name, 'size': path.stat().st_size,
                                  'state': 'uploaded', 'digest': None if self.omit_digest else 'sha256:' + hashlib.sha256(path.read_bytes()).hexdigest()})

  def download(self, asset_id: int, destination: Path, max_bytes: int) -> None:
    asset = next(item for item in self.release['assets'] if item['id'] == asset_id)
    shutil.copyfile(self.store / asset['name'], destination)
    if self.bad_download:
      destination.write_bytes(b'truncated')


def publisher():
  path = TOOLS / 'publish_runtime_release.py'
  assert path.exists(), 'tracked exact-source publisher is missing'
  spec = importlib.util.spec_from_file_location('publish_runtime_release', path)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


@pytest.fixture
def prepared(request: pytest.FixtureRequest):
  source = request.getfixturevalue('release_source')
  assert package_cli(source).returncode == 0
  root, _, _, assets = source
  return root, assets, OwnedGithub(root, assets.with_name('owned-remote'))


def test_verified_assets_are_uploaded_as_draft_then_published(prepared) -> None:
  root, assets, remote = prepared
  result = publisher().publish(root, assets, remote)
  assert result['source_commit'] == remote.commit
  assert result['download_verified'] is True
  assert remote.release['draft'] is False
  assert remote.release['prerelease'] is True
  assert remote.release['target_commitish'] == remote.commit
  assert remote.release['make_latest'] == 'false'
  assert {asset['name'] for asset in remote.release['assets']} == {path.name for path in assets.iterdir()}
  assert remote.mutations[0] == ('POST', 'releases')
  assert remote.mutations[-1] == ('PATCH', 'releases/10')


@pytest.mark.parametrize('fault', ['wrong-repo', 'wrong-head', 'branch', 'event', 'run-failed', 'job-skipped', 'required-failed', 'missing-workflow',
                                  'wrong-tag', 'dirty', 'published'])
def test_publisher_fails_closed_before_remote_mutation(prepared, fault: str) -> None:
  root, assets, remote = prepared
  if fault == 'wrong-repo':
    remote.full_name = 'bin9208/openpilot'
  elif fault in ('wrong-head', 'branch', 'event', 'run-failed'):
    key, value = {'wrong-head': ('head_sha', 'f' * 40), 'branch': ('head_branch', 'main'),
                  'event': ('event', 'pull_request'), 'run-failed': ('conclusion', 'failure')}[fault]
    remote.runs[0][key] = value
  elif fault == 'job-skipped':
    remote.jobs[1][0]['conclusion'] = 'skipped'
  elif fault == 'required-failed':
    remote.checks[0]['conclusion'] = 'failure'
  elif fault == 'missing-workflow':
    remote.runs.pop()
  elif fault == 'wrong-tag':
    remote.tag_sha = 'f' * 40
  elif fault == 'dirty':
    (root / 'source.txt').write_text('dirty')
  elif fault == 'published':
    remote.release = {'id': 10, 'tag_name': f'runtime-{remote.commit}', 'draft': False, 'prerelease': True,
                      'target_commitish': remote.commit, 'assets': []}
  with pytest.raises(ReleaseError):
    publisher().publish(root, assets, remote)
  assert remote.mutations == []


def test_check_only_is_read_only(prepared) -> None:
  root, assets, remote = prepared
  result = publisher().publish(root, assets, remote, check_only=True)
  assert result['source_commit'] == remote.commit
  assert remote.release is None
  assert remote.mutations == []


def test_interrupted_matching_draft_resumes_without_overwriting_assets(prepared) -> None:
  root, assets, remote = prepared
  remote.bad_download = True
  with pytest.raises(ReleaseError):
    publisher().publish(root, assets, remote)
  assert remote.release['draft'] is True
  assert not any(method == 'PATCH' for method, _ in remote.mutations)
  prior_uploads = len([method for method, _ in remote.mutations if method == 'UPLOAD'])
  remote.bad_download = False
  result = publisher().publish(root, assets, remote)
  assert result['download_verified'] is True
  assert len([method for method, _ in remote.mutations if method == 'UPLOAD']) == prior_uploads


def test_draft_with_mismatching_asset_is_never_overwritten(prepared) -> None:
  root, assets, remote = prepared
  remote.bad_download = True
  with pytest.raises(ReleaseError):
    publisher().publish(root, assets, remote)
  remote.bad_download = False
  remote.release['assets'][0]['digest'] = 'sha256:' + 'f' * 64
  mutations = len(remote.mutations)
  with pytest.raises(ReleaseError):
    publisher().publish(root, assets, remote)
  assert len(remote.mutations) == mutations


def test_missing_github_digest_requires_download_hash_verification(prepared) -> None:
  root, assets, remote = prepared
  remote.omit_digest = True
  assert publisher().publish(root, assets, remote)['download_verified'] is True


def test_publisher_low_disk_does_not_create_remote_draft(prepared, monkeypatch) -> None:
  root, assets, remote = prepared
  usage = shutil.disk_usage(assets)
  monkeypatch.setattr(shutil, 'disk_usage', lambda _: type(usage)(usage.total, usage.used, 1))
  with pytest.raises(ReleaseError, match='25 GiB'):
    publisher().publish(root, assets, remote)
  assert remote.release is None
  assert remote.mutations == []

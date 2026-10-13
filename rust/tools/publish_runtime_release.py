#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Publish verified local runtime assets as an exact-SHA GitHub prerelease.

Run: python3 rust/tools/publish_runtime_release.py --root ROOT --assets DIRECTORY
Use --check-only to validate local assets and exact-SHA CI with no remote writes.
All remote operations are pinned to github.com/bin9208/openpilot-rust.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
from typing import Final, Protocol

from package_runtime_release import validate_assets
from runtime_release_support import (
  JSON, REPOSITORY, ReleaseError, clean_source, disk_guard, hex_digest, integer, mapping, require, sequence, sha256, string,
)

GATES: Final = {
  '.github/workflows/rust.yml': {'rust checks', 'rust aarch64 build'},
  '.github/workflows/fast-checks.yml': {'fast checks'},
  '.github/workflows/integration.yml': {'integration gate'},
  '.github/workflows/user-docs.yaml': {'check mapped user docs'},
}
BASE: Final = f'repos/{REPOSITORY}'


class Github(Protocol):
  def api(self, endpoint: str, *, method: str = 'GET', body: dict[str, JSON] | None = None,
          missing: bool = False, paginate: bool = False) -> JSON: ...
  def upload(self, tag: str, path: Path) -> None: ...
  def download(self, asset_id: int, destination: Path, max_bytes: int) -> None: ...


class GhGithub:
  """Use the existing gh authentication with fixed-host/repository arguments."""

  def api(self, endpoint: str, *, method: str = 'GET', body: dict[str, JSON] | None = None,
          missing: bool = False, paginate: bool = False) -> JSON:
    command = ['gh', 'api', '--hostname', 'github.com', endpoint, '--method', method, '-H', 'Accept: application/vnd.github+json']
    if paginate:
      command.extend(['--paginate', '--slurp'])
    if missing:
      command.append('--include')
    if body is not None:
      command.extend(['--input', '-'])
    result = subprocess.run(command, input=json.dumps(body) if body is not None else None, capture_output=True, text=True, check=False)
    output = result.stdout
    if missing:
      status_line, _, _ = output.partition('\n')
      if status_line.startswith('HTTP/') and status_line.split()[1] == '404':
        return None
      _, separator, output = output.partition('\n\n')
      require(bool(separator), f'missing HTTP response headers: {endpoint}')
    require(result.returncode == 0, f'GitHub API failed: {endpoint}: {result.stderr.strip()}')
    return json.loads(output)

  def upload(self, tag: str, path: Path) -> None:
    subprocess.run(['gh', 'release', 'upload', tag, str(path.resolve()), '--repo', f'https://github.com/{REPOSITORY}'], check=True)

  def download(self, asset_id: int, destination: Path, max_bytes: int) -> None:
    """Stream authenticated assets into a bounded owned file, including drafts."""
    command = ['gh', 'api', '--hostname', 'github.com', f'{BASE}/releases/assets/{asset_id}', '-H', 'Accept: application/octet-stream']
    with destination.open('xb') as target, tempfile.TemporaryFile() as errors:
      with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors) as process:
        require(process.stdout is not None, 'missing asset download pipe')
        count = 0
        if process.stdout is not None:
          with process.stdout:
            while chunk := process.stdout.read(1024**2):
              count += len(chunk)
              if count > max_bytes:
                process.kill()
                process.wait()
                raise ReleaseError('download exceeds recorded asset size')
              target.write(chunk)
        require(process.wait() == 0, f'GitHub asset download failed: {asset_id}')
        require(count == max_bytes, f'truncated GitHub asset download: {asset_id}')


def page_rows(github: Github, endpoint: str, key: str | None = None) -> list[dict[str, JSON]]:
  pages = sequence(github.api(endpoint, paginate=True))
  rows: list[dict[str, JSON]] = []
  for page in pages:
    values = sequence(mapping(page).get(key)) if key is not None else sequence(page)
    rows.extend(mapping(value) for value in values)
  return rows


def succeeded(record: dict[str, JSON]) -> bool:
  return record.get('status') == 'completed' and record.get('conclusion') == 'success'


def ci_evidence(github: Github, commit: str) -> dict[str, JSON]:
  """Require protected contexts and latest matching dev push workflow gates."""
  require(mapping(github.api(BASE)).get('full_name') == REPOSITORY, 'wrong GitHub repository identity')
  policy = mapping(github.api(f'{BASE}/branches/dev/protection/required_status_checks'))
  required = {string(name) for name in sequence(policy.get('contexts'))}
  configured = [mapping(check) for check in sequence(policy.get('checks'))]
  required.update(string(check.get('context')) for check in configured)
  require(set().union(*GATES.values()).issubset(required), 'required dev protection checks missing')
  runs = page_rows(github, f'{BASE}/actions/runs?event=push&branch=dev&head_sha={commit}&per_page=100', 'workflow_runs')
  urls: list[JSON] = []
  for workflow, names in GATES.items():
    candidates = [run for run in runs if run.get('path') == workflow]
    require(bool(candidates), f'missing exact-SHA dev push workflow: {workflow}')
    run = max(candidates, key=lambda run: integer(run.get('id')))
    require(run.get('head_sha') == commit and run.get('head_branch') == 'dev' and run.get('event') == 'push'
            and mapping(run.get('repository')).get('full_name') == REPOSITORY and succeeded(run), f'incomplete/failed dev push workflow: {workflow}')
    jobs = page_rows(github, f'{BASE}/actions/runs/{integer(run.get("id"))}/jobs?filter=latest&per_page=100', 'jobs')
    for name in names:
      matching = [job for job in jobs if job.get('name') == name]
      require(len(matching) == 1 and succeeded(matching[0]) and matching[0].get('head_sha') == commit, f'failed/missing exact-SHA gate: {name}')
    urls.append(string(run.get('html_url')))
  checks = page_rows(github, f'{BASE}/commits/{commit}/check-runs?filter=latest&per_page=100', 'check_runs')
  statuses = page_rows(github, f'{BASE}/commits/{commit}/status?per_page=100', 'statuses')
  for name in required:
    matches = [check for check in checks if check.get('name') == name]
    app_ids = {check.get('app_id') for check in configured if check.get('context') == name and check.get('app_id') not in (None, -1)}
    if app_ids:
      matches = [check for check in matches if mapping(check.get('app')).get('id') in app_ids]
    if matches:
      require(succeeded(max(matches, key=lambda check: integer(check.get('id')))), f'required check failed: {name}')
    else:
      matches = [status for status in statuses if status.get('context') == name]
      require(not app_ids and bool(matches) and max(matches, key=lambda status: integer(status.get('id'))).get('state') == 'success',
              f'required status missing/failed: {name}')
  return {'required_checks': sorted(required), 'dev_push_runs': urls}


def tag_commit(github: Github, tag: str) -> str | None:
  reference = github.api(f'{BASE}/git/ref/tags/{tag}', missing=True)
  if reference is None:
    return None
  target = mapping(mapping(reference).get('object'))
  for _ in range(4):
    if target.get('type') == 'commit':
      return hex_digest(target.get('sha'), 40)
    require(target.get('type') == 'tag', 'unexpected release tag target')
    target = mapping(mapping(github.api(f'{BASE}/git/tags/{hex_digest(target.get("sha"), 40)}')).get('object'))
  raise ReleaseError('release tag nesting exceeds limit')


def verify_remote_assets(github: Github, release_id: int, local: dict[str, dict[str, JSON]], assets: Path,
                         *, complete: bool) -> set[str]:
  """Verify size, available digest and downloaded bytes for every present asset."""
  remote = page_rows(github, f'{BASE}/releases/{release_id}/assets?per_page=100')
  seen: set[str] = set()
  for asset in remote:
    name = string(asset.get('name'))
    require(name in local and name not in seen and asset.get('state') == 'uploaded', f'unexpected/duplicate/incomplete uploaded asset: {name}')
    expected = local[name]
    require(asset.get('size') == expected['bytes'], f'uploaded asset size mismatch: {name}')
    require(asset.get('digest') in (None, '', 'sha256:' + string(expected['sha256'])), f'uploaded asset digest mismatch: {name}')
    size = integer(expected['bytes'])
    disk_guard(assets, size + 64 * 1024**2)
    with tempfile.TemporaryDirectory(dir=assets.parent, prefix='.runtime-download-') as directory:
      downloaded = Path(directory) / name
      github.download(integer(asset.get('id')), downloaded, size)
      require(downloaded.stat().st_size == size and sha256(downloaded) == expected['sha256'], f'downloaded asset size/hash mismatch: {name}')
    seen.add(name)
  require(not complete or seen == local.keys(), 'missing uploaded release assets')
  return seen


def publish(root: Path, assets: Path, github: Github, *, check_only: bool = False) -> dict[str, JSON]:
  descriptor = validate_assets(root, assets)
  commit = hex_digest(descriptor.get('source_commit'), 40)
  ci = ci_evidence(github, commit)
  tag = f'runtime-{commit}'
  reference = tag_commit(github, tag)
  require(reference in (None, commit), 'existing release tag points to different source')
  local: dict[str, dict[str, JSON]] = {path.name: {'sha256': sha256(path), 'bytes': path.stat().st_size} for path in sorted(assets.iterdir())}
  identity = json.dumps({'repository': REPOSITORY, 'source_commit': commit, 'assets': local}, sort_keys=True, indent=2)
  body = f'Exact-source runtime release identity:\n\n```json\n{identity}\n```\n'
  releases = [release for release in page_rows(github, f'{BASE}/releases?per_page=100') if release.get('tag_name') == tag]
  require(len(releases) <= 1, 'duplicate release identity')
  release = releases[0] if releases else None
  if release is not None:
    require(release.get('draft') is True, 'refusing to overwrite published runtime release')
    require(release.get('target_commitish') == commit and release.get('prerelease') is True and release.get('body') == body,
            'existing draft identity/assets differ')
    existing = verify_remote_assets(github, integer(release.get('id')), local, assets, complete=False)
  else:
    existing = set()
  if check_only:
    return {'source_commit': commit, 'tag': tag, 'ci': ci, 'check_only': True}
  disk_guard(assets, max(integer(details['bytes']) for details in local.values()) + 64 * 1024**2)
  require(clean_source(root)[0] == commit, 'source changed before publication')
  if release is None:
    release = mapping(github.api(f'{BASE}/releases', method='POST', body={'tag_name': tag, 'target_commitish': commit, 'name': tag,
                      'body': body, 'draft': True, 'prerelease': True, 'make_latest': 'false'}))
    require(release.get('draft') is True and release.get('tag_name') == tag and release.get('target_commitish') == commit,
            'created draft identity mismatch')
  release_id = integer(release.get('id'))
  for name in local:
    if name not in existing:
      github.upload(tag, assets / name)
  verify_remote_assets(github, release_id, local, assets, complete=True)
  current = mapping(github.api(f'{BASE}/releases/{release_id}'))
  require(current.get('draft') is True and current.get('body') == body and current.get('target_commitish') == commit
          and current.get('tag_name') == tag and current.get('prerelease') is True, 'draft changed before publication')
  require(tag_commit(github, tag) in (None, commit), 'release tag changed before publication')
  require(clean_source(root)[0] == commit, 'source changed before publishing verified draft')
  require(all((assets / name).stat().st_size == details['bytes'] and sha256(assets / name) == details['sha256']
              for name, details in local.items()), 'local assets changed before publishing verified draft')
  published = mapping(github.api(f'{BASE}/releases/{release_id}', method='PATCH', body={'draft': False, 'prerelease': True, 'make_latest': 'false'}))
  require(published.get('draft') is False and published.get('prerelease') is True and tag_commit(github, tag) == commit, 'published release identity mismatch')
  verify_remote_assets(github, release_id, local, assets, complete=True)
  return {'source_commit': commit, 'tag': tag, 'release_url': published.get('html_url'), 'ci': ci, 'assets': local, 'download_verified': True}


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--assets', type=Path, required=True)
  parser.add_argument('--check-only', action='store_true')
  args = parser.parse_args()
  try:
    result = publish(args.root.resolve(strict=True), args.assets.resolve(strict=True), GhGithub(), check_only=args.check_only)
  except (ReleaseError, OSError, EOFError, tarfile.TarError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
    parser.exit(1, f'runtime publication: {error}\n')
  print(json.dumps(result, sort_keys=True, indent=2))


if __name__ == '__main__':
  main()

"""Temporary GitHub-hosted original-only capture; never imported by maintained code."""
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import tempfile

SOURCE_PINS = {
  'rust/tools/updated_fixtures.py': '76607e02520f383227264257de0ed8505b9dc12e462227b176f2c919dfec2d60',
  'rust/tools/check_updated.py': 'bfa1686e464e5457b9a9bdc66b954e81f5994db24860d7f7b2869999f4e29b7d',
  'rust/tools/check_updated_daemon.py': '82be3830f8e468c32abe583c197cdf2777cd5fb748a795a8c70247a7dcd00059',
  'rust/tools/updated_source.py': 'bb52c5227c08d70cace263c84778d9a8d87cf92ea6e30f6770d57788da1ebc80',
}
root = Path(os.environ['GITHUB_WORKSPACE']).resolve()
out = Path(sys.argv[1]).resolve()
out.mkdir(parents=True, exist_ok=False)
for name, expected in SOURCE_PINS.items():
  assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected, name
sys.path.insert(0, str(root / 'rust/tools'))
from updated_fixtures import git, normalize, seed, setup  # noqa: E402; original-only cloud execution


def descriptor(path):
  mode = path.lstat().st_mode
  result = {'mode': stat.S_IMODE(mode)}
  if path.is_symlink():
    result.update(kind='symlink', target=os.readlink(path))
  elif path.is_file():
    raw = path.read_bytes()
    result.update(kind='file', sha256=hashlib.sha256(raw).hexdigest(), hex=raw.hex())
  elif path.is_dir():
    result.update(kind='directory')
  else:
    raise AssertionError(path)
  return result


def normalize_paths(value, roots):
  if isinstance(value, str):
    for path, label in roots:
      value = value.replace(str(path), label)
    return value
  if isinstance(value, list):
    return [normalize_paths(item, roots) for item in value]
  if isinstance(value, dict):
    return {key: normalize_paths(item, roots) for key, item in value.items()}
  return value

with tempfile.TemporaryDirectory(prefix='updated-fixture-original-') as location:
  directory = Path(location)
  remote, previous = seed(directory)
  remote_head = git(remote, 'rev-parse', 'HEAD')
  assert previous != remote_head
  version_names = ['common/version.h', 'RELEASES.md', 'launch_env.sh', 'build.json',
                   'version-link', 'openpilot/system/hardware/tici/agnos.json',
                   'openpilot/system/hardware/tici/agnos-tici.json']
  remote_entries = {name: descriptor(remote / name) for name in version_names}
  root_paths = [(directory, '<TEMP>')]
  lanes = {}
  for lane in ('source', 'native'):
    owned = directory / lane
    config, env = setup(owned, remote, previous, Path('/usr/bin/true'))
    names = ['base/common/version.h', 'base/RELEASES.md', 'base/launch_env.sh',
             'base/build.json', 'base/version-link',
             'data/params/d', 'tmp', 'sys/firmware/devicetree/base/model',
             'bin/sudo', 'bin/git', 'bin/bash', 'bin/ionice', 'bin/find']
    entry = {name: descriptor(owned / name) for name in names}
    lanes[lane] = {
      'config': config,
      'environment': {key: env.get(key) for key in ('PATH','UPDATED_FIXTURE_ROOT','GIT_CONFIG_NOSYSTEM','GIT_CONFIG_GLOBAL','TZ','OPENPILOT_PREFIX')},
      'entries': entry,
      'base_head': git(owned / 'base', 'rev-parse', 'HEAD'),
      'base_branch': git(owned / 'base', 'branch', '--show-current'),
      'base_status': git(owned / 'base', 'status', '--porcelain=v1'),
    }
    assert lanes[lane]['base_head'] == previous
  errors = {}
  for label, operation in {
    'seed_existing_remote': lambda: seed(directory),
    'setup_existing_root': lambda: setup(directory / 'source', remote, previous, Path('/usr/bin/true')),
    'git_missing_ref': lambda: git(remote, 'rev-parse', '--verify', 'refs/heads/not-a-fixture-branch'),
  }.items():
    try:
      operation()
    except Exception as error:
      errors[label] = {'exception': type(error).__name__, 'returncode': getattr(error, 'returncode', None)}
    else:
      raise AssertionError(f'expected original error: {label}')
  sample = {'path': str(directory / 'native'), 'params': {'One': (b'pre-' + str(directory / 'native').encode() + b'-post').hex()}}
  capture = {
    'source_commit': os.environ['UPDATED_ORIGINAL_SOURCE_COMMIT'],
    'workflow_commit': os.environ['GITHUB_SHA'],
    'python_executable': sys.executable,
    'python_version': sys.version,
    'git_version': git(remote, '--version'),
    'source_sha256': SOURCE_PINS,
    'previous_commit': previous,
    'remote_head': remote_head,
    'remote_branches': git(remote, 'branch', '--format=%(refname:short)'),
    'remote_log': git(remote, 'log', '--format=%H%x09%an%x09%ae%x09%ad%x09%s', '--date=iso-strict'),
    'remote_entries': remote_entries,
    'lanes': lanes,
    'errors': errors,
    'normalize_input': sample,
    'normalize_output': normalize(sample, directory / 'native'),
  }
  capture = normalize_paths(capture, root_paths)
  payload = json.dumps(capture, sort_keys=True, indent=2).encode() + b'\n'
  (out / 'original.json').write_bytes(payload)
  (out / 'original.sha256').write_text(hashlib.sha256(payload).hexdigest() + '  original.json\n')

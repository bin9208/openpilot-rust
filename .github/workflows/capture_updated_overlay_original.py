"""Temporary GitHub-hosted original-only overlay shim capture. No host sudo/mount."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

PINS = {
  'rust/tools/updated_fixtures.py': '76607e02520f383227264257de0ed8505b9dc12e462227b176f2c919dfec2d60',
  'rust/tools/check_updated.py': 'bfa1686e464e5457b9a9bdc66b954e81f5994db24860d7f7b2869999f4e29b7d',
  'rust/tools/check_updated_daemon.py': '82be3830f8e468c32abe583c197cdf2777cd5fb748a795a8c70247a7dcd00059',
}
repo = Path(os.environ['GITHUB_WORKSPACE']).resolve()
out = Path(sys.argv[1]).resolve()
out.mkdir(parents=True, exist_ok=False)
for name, digest in PINS.items():
  assert hashlib.sha256((repo / name).read_bytes()).hexdigest() == digest, name
sys.path.insert(0, str(repo / 'rust/tools'))
from updated_fixtures import seed, setup  # noqa: E402: original-only cloud capture


def describe(path):
  mode = path.lstat().st_mode
  result = {'mode': stat.S_IMODE(mode)}
  if path.is_symlink():
    result.update(kind='symlink', target=os.readlink(path))
  elif path.is_file():
    raw = path.read_bytes()
    result.update(kind='file', raw_hex=raw.hex(), sha256=hashlib.sha256(raw).hexdigest())
  elif path.is_dir():
    result.update(kind='directory')
  else:
    result.update(kind='other')
  return result


def tree(root):
  result = {}
  def visit(path):
    for child in sorted(path.iterdir()):
      rel = child.relative_to(root).as_posix()
      if '.git' in child.relative_to(root).parts:
        continue  # Git objects are captured by the prior immutable source receipt.
      result[rel] = describe(child)
      if child.is_dir() and not child.is_symlink():
        visit(child)
  visit(root)
  return result


def log_receipt(root):
  log = root / 'overlay-commands.jsonl'
  if not log.exists():
    return {'exists': False}
  raw = log.read_bytes()
  return {
    'exists': True,
    'raw_hex': raw.hex(),
    'sha256': hashlib.sha256(raw).hexdigest(),
    'normalized_root_text': raw.replace(str(root).encode(), b'<ROOT>').decode('utf-8'),
  }


def action(label, root, env, args):
  executable = root / 'bin/sudo'
  command = [str(executable), *args]
  observed = subprocess.run(command, env=env, capture_output=True, timeout=15)
  return {
    'label': label,
    'argv': command,
    'exit': observed.returncode,
    'stdout_hex': observed.stdout.hex(),
    'stderr_hex': observed.stderr.hex(),
    'log': log_receipt(root),
    'tree': tree(root),
  }

with tempfile.TemporaryDirectory(prefix='updated-overlay-original-', dir=os.environ['RUNNER_TEMP']) as location:
  work = Path(location)
  remote, previous = seed(work)
  scenarios = {}
  for lane in ('success', 'repeat', 'symlink-outside', 'nested-symlink-outside', 'errors'):
    root = work / lane
    config, environment = setup(root, remote, previous, Path('/usr/bin/true'))
    stage = Path(config['paths']['staging'])
    workdir = root / 'data/work'
    workdir.mkdir()
    options = f'lowerdir={root / "base"},upperdir={root / "data/upper"},workdir={workdir}'
    mount = ['mount', '-t', 'overlay', '-o', options, 'none', str(stage)]
    captures = []
    if lane == 'success':
      captures.append(action('mount', root, environment, mount))
      captures.append(action('chmod', root, environment, ['chmod', '755', str(stage / 'launch_env.sh')]))
      captures.append(action('umount', root, environment, ['umount', '-l', str(stage)]))
      captures.append(action('rm', root, environment, ['rm', '-rf', str(stage)]))
    elif lane == 'repeat':
      captures.append(action('first-mount', root, environment, mount))
      captures.append(action('second-mount', root, environment, mount))
    elif lane == 'symlink-outside':
      outside = work / 'private-outside'
      outside.mkdir()
      (outside / 'sentinel').write_bytes(b'private sibling, never a host path\n')
      stage.symlink_to(outside, target_is_directory=True)
      captures.append(action('mount-to-sibling-symlink', root, environment, mount))
      scenarios['private_outside_after'] = tree(outside)
    elif lane == 'nested-symlink-outside':
      outside = work / 'private-nested-outside'
      outside.mkdir()
      target = outside / 'version.h'
      target.write_bytes(b'private original contents; outside the fixture root\n')
      stage.joinpath('common').mkdir(parents=True)
      stage.joinpath('common/version.h').symlink_to(target)
      scenarios['nested_outside_before'] = describe(target)
      captures.append(action('mount-to-existing-tree-with-nested-symlink', root, environment, mount))
      scenarios['nested_outside_after'] = describe(target)
    else:
      captures.append(action('unknown-command', root, environment, ['echo', 'forbidden']))
      captures.append(action('outside-root-rm', root, environment, ['rm', '-rf', str(work / 'private-outside')]))
      captures.append(action('missing-overlay-option', root, environment, ['mount', '-t', 'overlay', '-o', f'lowerdir={root / "base"}', 'none', str(stage)]))
      captures.append(action('short-mount', root, environment, ['mount', '-t', 'overlay']))
    scenarios[lane] = captures
  payload = {
    'source_commit': os.environ['UPDATED_ORIGINAL_SOURCE_COMMIT'],
    'workflow_commit': os.environ['GITHUB_SHA'],
    'source_sha256': PINS,
    'python_executable': sys.executable,
    'python_version': sys.version,
    'temp_root': str(work),
    'scenarios': scenarios,
    'scope': 'unchanged emitted Python sudo shim, private temp roots only; no host sudo or mount',
  }
  raw = json.dumps(payload, indent=2, sort_keys=True).encode() + b'\n'
  (out / 'original-overlay.json').write_bytes(raw)
  (out / 'original-overlay.sha256').write_text(hashlib.sha256(raw).hexdigest() + '  original-overlay.json\n')

"""Owned Git repositories and a PATH-selected overlay command boundary."""

import json
import os
import shutil
import subprocess
import sys


def git(root, *args):
  return (
    subprocess.check_output(
      ['/usr/bin/git', *args], cwd=root, stderr=subprocess.STDOUT, env=dict(os.environ, GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null')
    )
    .decode()
    .strip()
  )


def seed(directory):
  remote = directory / 'remote'
  remote.mkdir()
  git(remote, 'init', '-b', 'dev')
  git(remote, 'config', 'user.email', 'fixture@example.invalid')
  git(remote, 'config', 'user.name', 'Owned updater fixture')
  for version in ['1.0', '2.0']:
    (remote / 'common').mkdir(exist_ok=True)
    (remote / 'common/version.h').write_text(f'#define COMMA_VERSION "{version}"\n')
    (remote / 'RELEASES.md').write_text(f'{version}\n===\n* verified & staged\n  * nested\n\nolder release')
    (remote / 'launch_env.sh').write_text('export AGNOS_VERSION="fixture-os"\n')
    (remote / 'launch_env.sh').chmod(0o755)
    (remote / 'build.json').write_text(json.dumps({'channel': 'dev', 'openpilot': {'version': version}}))
    if not (remote / 'version-link').exists():
      (remote / 'version-link').symlink_to('common/version.h')
    agnos = remote / 'openpilot/system/hardware/tici'
    agnos.mkdir(parents=True, exist_ok=True)
    for name in ['agnos.json', 'agnos-tici.json']:
      (agnos / name).write_text('[]')
    git(remote, 'add', '.')
    env = dict(
      os.environ, GIT_AUTHOR_DATE='2026-01-01T00:00:00Z', GIT_COMMITTER_DATE='2026-01-01T00:00:00Z', GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null'
    )
    subprocess.run(['/usr/bin/git', 'commit', '-m', version], cwd=remote, env=env, check=True, capture_output=True)
    if version == '1.0':
      previous = git(remote, 'rev-parse', 'HEAD')
  git(remote, 'branch', 'release2')
  git(remote, 'branch', 'release-tizi')
  return remote, previous


def setup(root, remote, previous, launcher):
  root.mkdir()
  base = root / 'base'
  git(root, 'clone', str(remote), str(base))
  git(base, 'reset', '--hard', previous)
  params = root / 'data/params/d'
  params.mkdir(parents=True)
  (root / 'tmp').mkdir()
  (root / 'sys/firmware/devicetree/base').mkdir(parents=True)
  (root / 'sys/firmware/devicetree/base/model').write_text('comma c3\x00')
  bindir = root / 'bin'
  bindir.mkdir()
  # The fixture never invokes sudo, mount, umount, or recursive removal outside root.
  sudo = bindir / 'sudo'
  sudo.write_text(f'''#!{sys.executable}
import json, os, pathlib, shutil, sys
root = pathlib.Path(os.environ['UPDATED_FIXTURE_ROOT']).resolve()
args = sys.argv[1:]
def owned(path):
 p = pathlib.Path(path).absolute()
 assert p.is_relative_to(root) and p != root, p
 return p
with (root/'overlay-commands.jsonl').open('a') as f: f.write(json.dumps(args)+'\\n')
if args[:2] == ['rm','-rf'] and len(args)==3:
 p=owned(args[2])
 if p.exists(): shutil.rmtree(p)
elif args[:4] == ['mount','-t','overlay','-o'] and len(args)==7:
 options=dict(item.split('=',1) for item in args[4].split(','))
 source=owned(options['lowerdir']); target=owned(args[6]); work=owned(options['workdir'])/'work'
 shutil.copytree(source,target,dirs_exist_ok=True,symlinks=True)
 work.mkdir()
elif args[:2] == ['chmod','755'] and len(args)==3: owned(args[2]).chmod(0o755)
elif args[:2] == ['umount','-l'] and len(args)==3: owned(args[2])
else: raise AssertionError(args)
''')
  sudo.chmod(0o755)
  for name in ['git', 'bash', 'ionice', 'find']:
    target = shutil.which(name)
    assert target, name
    (bindir / name).symlink_to(target)
  env = dict(os.environ, PATH=str(bindir) + ":/usr/bin:/bin", UPDATED_FIXTURE_ROOT=str(root), GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null', TZ='UTC')
  env.pop('OPENPILOT_PREFIX', None)
  config = {
    'paths': {'base': str(base), 'staging': str(root / 'data/stage'), 'lock': str(root / 'tmp/updated.lock'), 'system_root': str(root)},
    'launcher': str(launcher),
    'now': 1790812800.0,
    'device': 'pc',
    'os_version': 'fixture-os',
    'agnos': False,
    'steps': [],
  }
  return config, env


def normalize(value, root):
  if isinstance(value, dict):
    result = {}
    for key, item in value.items():
      if key == 'params':
        result[key] = {}
        for name, raw in item.items():
          data = bytes.fromhex(raw)
          result[key][name] = data.replace(str(root).encode(), b'<ROOT>').hex()
      else:
        result[key] = normalize(item, root)
    return result
  if isinstance(value, list):
    return [normalize(item, root) for item in value]
  if isinstance(value, str):
    return value.replace(str(root), '<ROOT>')
  return value

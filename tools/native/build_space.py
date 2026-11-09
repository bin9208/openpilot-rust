"""Reclaim obsolete installed packages, then old logs, before a native build.

Only the original installer's package store and logger's segment directories
are eligible. Compiler/input caches stay available for incremental builds.
"""
from contextlib import contextmanager
import errno
import fcntl
import os
from pathlib import Path
import re
import shutil
import stat
import sys

PACKAGE = re.compile(r'gen-[0-9a-f]{40}-[0-9a-f]{64}-[A-Za-z0-9]{1,32}')
SEGMENT = re.compile(r'(?:[0-9a-f]{8}--[0-9a-f]{10}|\d{4}-\d{2}-\d{2}--\d{2}-\d{2}-\d{2})--[0-9]+')


def real_directory(path):
  return path.is_dir() and path.resolve() == path and not path.is_symlink()


def existing(path):
  while not path.exists():
    path = path.parent
  if not real_directory(path):
    raise ValueError('build-space filesystem path is redirected')
  return path


def shortfalls(machine):
  result = {}
  for name, budget in machine['resources']['filesystems'].items():
    path = existing(Path(name))
    required = budget['maximum_growth_bytes'] + budget['minimum_free_bytes']
    free = shutil.disk_usage(path).free
    if free < required:
      device = path.stat().st_dev
      if device not in result or required > result[device]['required']:
        result[device] = dict(path=path, required=required, free=free)
  return result


@contextmanager
def installer_lock(source):
  rust = source/'rust'
  if not real_directory(rust) or not real_directory(rust/'.runtime-packages'):
    yield False
    return
  fd = os.open(rust/'.runtime-install.lock', os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
  try:
    if not stat.S_ISREG(os.fstat(fd).st_mode):
      raise ValueError('runtime installer lock is not regular')
    try:
      fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      yield False
    else:
      yield True
  finally:
    os.close(fd)


def unused_packages(source, proc=Path('/proc')):
  store = source/'rust/.runtime-packages'
  if not real_directory(store):
    return []
  protected = set()
  active = source/'rust/.runtime-active'
  if os.path.lexists(active):
    target = Path(os.readlink(active))
    if target.parent != Path('.runtime-packages') or not PACKAGE.fullmatch(target.name):
      raise ValueError('cannot clean packages with an invalid active runtime link')
    protected.add(target.name)
  journal = source/'rust/.runtime-install-journal.json'
  # An interrupted installer must finish recovery before any package removal.
  if os.path.lexists(journal) or os.path.lexists(source/'rust/.runtime-install-rollback'):
    return []
  try:
    for process in proc.iterdir():
      if not process.name.isdigit():
        continue
      try:
        executable = Path(os.readlink(process/'exe').removesuffix(' (deleted)'))
      except FileNotFoundError:
        continue
      if executable.is_relative_to(store):
        protected.add(executable.relative_to(store).parts[0])
  except OSError as error:
    # Match the installer's conservative live-owner policy. Missing permissions
    # are not evidence that a package's sibling helpers are unused.
    print(f'native build space: package cleanup deferred: {error}', file=sys.stderr)
    return []
  packages = sorted((p for p in store.iterdir() if PACKAGE.fullmatch(p.name) and real_directory(p)),
                    key=lambda p: p.stat().st_mtime_ns, reverse=True)
  # Keep one previous package in addition to active/live packages for recovery.
  previous = next((p for p in packages if p.name not in protected), None)
  if previous is not None:
    protected.add(previous.name)
  return [p for p in reversed(packages) if p.name not in protected]


def log_sort(path):
  name = path.name
  return (not name.startswith('2024-'), *[part.rjust(10, '0') for part in name.rsplit('--', 1)])


def old_segments(root):
  if not real_directory(root):
    return []
  segments = sorted((p for p in root.iterdir() if SEGMENT.fullmatch(p.name) and real_directory(p)), key=log_sort)
  protected = {segments[-1].name} if segments else set()
  for path in segments:
    try:
      preserve = os.getxattr(path, 'user.preserve', follow_symlinks=False) == b'1'
    except OSError as error:
      if error.errno != errno.ENODATA:
        raise  # Unknown retention state must not authorize log deletion.
      preserve = False
    if preserve:
      route, _, number = path.name.rpartition('--')
      protected.update(f'{route}--{i}' for i in range(max(0, int(number)-2), int(number)+1))
  return [p for p in segments if p.name not in protected]


def prepare(machine, source, observe, *, allow_logs=True, proc=Path('/proc')):
  """Called under source-driver.lock, before source copying/input preparation.

  Existing-runtime boots may reclaim old packages, but never logs merely to
  satisfy the capacity of a build they do not need to run.
  """
  if not shortfalls(machine):
    return
  log_root = Path(machine['stores']['params']).parent/'media/0/realdata'
  before = {device: row['free'] for device, row in shortfalls(machine).items()}

  def remove(path, logs=False):
    observe()
    if not real_directory(path) or path.stat().st_dev not in shortfalls(machine):
      return
    if logs:
      # Re-read bookmarks and locks immediately before each removal. Do not
      # clear even stale recorder/uploader lock files during migration.
      if path not in old_segments(log_root) or any(p.name.endswith('.lock') for p in path.iterdir()):
        return
    try:
      shutil.rmtree(path)
    except OSError as error:
      print(f'native build space: retained {path}: {error}', file=sys.stderr)
      return
    print(f'native build space: removed {path}', file=sys.stderr)
    observe()

  with installer_lock(source) as locked:
    if locked:
      for path in unused_packages(source, proc):
        if not shortfalls(machine):
          break
        remove(path)
  if allow_logs and shortfalls(machine):
    for path in old_segments(log_root):
      if not shortfalls(machine):
        break
      remove(path, logs=True)
  remaining = shortfalls(machine)
  for device, free in before.items():
    path = next(existing(Path(p)) for p in machine['resources']['filesystems']
                if existing(Path(p)).stat().st_dev == device)
    print(f'native build space: available {shutil.disk_usage(path).free/1024**3:.2f} GiB '
          f'(before {free/1024**3:.2f} GiB)', file=sys.stderr)
  if remaining and allow_logs:
    details = '; '.join(f"{row['path']}: available {row['free']/1024**3:.2f} GiB, "
        f"required {row['required']/1024**3:.2f} GiB, short {(row['required']-row['free'])/1024**3:.2f} GiB"
        for row in remaining.values())
    raise ValueError('insufficient build space after safe cleanup; protected files retained; '+details)

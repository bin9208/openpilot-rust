"""Content-addressed model-preparation wheels; no pip hooks or imports of wheels.

Python model preparation is retained. The Rust asset helper supplies verified,
counted downloads; this module admits exact wheel files into immutable prefixes.
"""
from __future__ import annotations

import argparse
import base64
import csv
import ctypes
import email.parser
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import tempfile
import time
from contextlib import contextmanager
from urllib.parse import urlsplit
import zipfile

METADATA_GROWTH = 8 * 1024**2
VERSIONS = {'numpy': '2.5.3', 'zstandard': '0.25.0'}


class InputError(RuntimeError):
  pass


def encoded(value):
  return (json.dumps(value, sort_keys=True, indent=2)+'\n').encode()


def sha(value):
  return hashlib.sha256(value).hexdigest()


def regular(path):
  fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
  if not stat.S_ISREG(os.fstat(fd).st_mode):
    os.close(fd)
    raise InputError('model input is not a regular file')
  return os.fdopen(fd, 'rb')


def digest(path, limit=None):
  with regular(path) as stream:
    if limit is None: return hashlib.file_digest(stream, 'sha256').hexdigest()
    checksum, left = hashlib.sha256(), limit+1
    while left:
      data = stream.read(min(left, 65536))
      if not data: break
      checksum.update(data)
      left -= len(data)
    if limit+1-left != limit: raise InputError('model input changed size during verification')
    return checksum.hexdigest()


def read_bytes(path, limit):
  with regular(path) as stream:
    if os.fstat(stream.fileno()).st_size > limit:
      raise InputError('model input exceeds its byte bound')
    data = stream.read(limit+1)
  if len(data) > limit:
    raise InputError('model input grew beyond its byte bound')
  return data


def unique_pairs(pairs):
  result = {}
  for key, value in pairs:
    if key in result:
      raise InputError('duplicate model input JSON key')
    result[key] = value
  return result


def read_json(path, limit=1024**2):
  return json.loads(read_bytes(path, limit), object_pairs_hook=unique_pairs)


def directory(path):
  path.mkdir(exist_ok=True)
  if not stat.S_ISDIR(path.lstat().st_mode):
    raise InputError('model input store requires real directories')


def normal(name):
  path = PurePosixPath(name)
  return (path != PurePosixPath('.') and 0 < len(name.encode()) <= 4096 and not path.is_absolute()
          and path.as_posix() == name and all(p not in ('..', '.git') for p in path.parts)
          and not any(c in name for c in '\\\x00\r\n\t'))


def validate(plan):
  if set(plan) != {'schema', 'target', 'abi', 'packages'} or plan['schema'] != 1:
    raise InputError('invalid model wheel plan schema')
  if plan['target'] != 'aarch64-unknown-linux-gnu' or plan['abi'] != 'cp312' or set(plan['packages']) != set(VERSIONS):
    raise InputError('model wheels require the complete pinned ARM cp312 selection')
  for name, p in plan['packages'].items():
    if set(p) != {'version', 'filename', 'asset', 'maximum_unpacked_bytes', 'maximum_members'}:
      raise InputError('invalid model wheel package fields')
    if p['version'] != VERSIONS[name] or not p['filename'].startswith(f'{name}-{p["version"]}-cp312-cp312-'):
      raise InputError('model wheel identity differs from its pinned package')
    if not p['filename'].endswith('aarch64.whl') or not normal(p['filename']) or '/' in p['filename']:
      raise InputError('model wheel filename is not a native ARM wheel')
    if not 0 < p['maximum_unpacked_bytes'] <= 256*1024**2 or not 0 < p['maximum_members'] <= 10000:
      raise InputError('invalid model wheel expansion estimate')
    a = p['asset']
    if set(a) != {'url', 'sha256', 'bytes'} or not re.fullmatch('[0-9a-f]{64}', a['sha256']) or not 0 < a['bytes'] <= 32*1024**2:
      raise InputError('invalid model wheel asset pin')
    url = urlsplit(a['url'])
    if (url.scheme != 'https' or url.netloc != 'files.pythonhosted.org' or url.query or url.fragment
        or not url.path.endswith('/'+p['filename'])):
      raise InputError('model wheel locator differs from the pinned public registry')


def package_identity(name, p):
  return dict(schema=1, role='openpilot-model-wheel-v1', name=name, version=p['version'],
              filename=p['filename'], asset=p['asset'])


def add(entries, path, entry):
  if not normal(path):
    raise InputError('unsafe model input tree member')
  for parent in reversed(PurePosixPath(path).parents):
    if parent == PurePosixPath('.'): continue
    key = parent.as_posix()
    if key in entries and entries[key] != {'kind': 'directory'}:
      raise InputError('model input tree overlaps a file')
    entries[key] = {'kind': 'directory'}
  if path in entries and entries[path] != entry:
    raise InputError('conflicting model input tree members')
  entries[path] = entry


def write(path, data):
  path.parent.mkdir(parents=True, exist_ok=True)
  fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
  with os.fdopen(fd, 'wb') as output:
    output.write(data)
    os.fchmod(output.fileno(), 0o444)
    output.flush()
    os.fsync(output.fileno())


def wheel_tree(name, p, blob, destination=None):
  # An immutable, bounded compressed byte buffer is both hashed and parsed.
  # Reopening a mutable cache after an earlier check cannot change this input.
  data = read_bytes(blob, p['asset']['bytes'])
  if len(data) != p['asset']['bytes'] or sha(data) != p['asset']['sha256']:
    raise InputError('model wheel differs from its authenticated compressed object')
  entries, hashes, metadata = {}, {}, {}
  dist = f'{name}-{p["version"]}.dist-info'
  allowed = {name, name+'.libs', dist, f'{name}-{p["version"]}.data'}
  total, notices = 0, 0
  with zipfile.ZipFile(io.BytesIO(data)) as archive:
    members = archive.infolist()
    if len(members) > p['maximum_members']:
      raise InputError('model wheel member count exceeds its estimate')
    seen = set()
    for info in members:
      path = info.filename.rstrip('/') if info.is_dir() else info.filename
      if info.orig_filename != info.filename or not normal(path) or path in seen or PurePosixPath(path).parts[0] not in allowed:
        raise InputError('model wheel has an unsafe, duplicate or unrelated member')
      seen.add(path)
      mode = info.external_attr >> 16
      kind = stat.S_IFMT(mode)
      if info.flag_bits & 1 or mode & 0o7000 or info.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
        raise InputError('unsupported model wheel encryption, mode or compression')
      target = 'site-packages/'+path
      if info.is_dir():
        if kind not in (0, stat.S_IFDIR) or info.file_size != 0:
          raise InputError('model wheel directory has unexpected type or bytes')
        add(entries, target, {'kind': 'directory'})
        if destination is not None: (destination/target).mkdir(parents=True, exist_ok=True)
        continue
      if kind not in (0, stat.S_IFREG) or info.file_size > 128*1024**2:
        raise InputError('model wheel contains a link, special or oversized file')
      total += info.file_size
      if total > p['maximum_unpacked_bytes']:
        raise InputError('model wheel exceeds its operation expansion estimate')
      selected = path in {dist+'/METADATA', dist+'/WHEEL', dist+'/RECORD'}
      if selected and info.file_size > 1024**2:
        raise InputError('model wheel metadata exceeds 1 MiB')
      output = None
      if destination is not None:
        (destination/target).parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(destination/target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        output = os.fdopen(fd, 'wb')
      count, content, header, checksum = 0, bytearray(), bytearray(), hashlib.sha256()
      try:
        with archive.open(info) as member:
          while chunk := member.read(65536):
            count += len(chunk)
            if count > info.file_size: raise InputError('model wheel member grew')
            checksum.update(chunk)
            if len(header) < 64: header.extend(chunk[:64-len(header)])
            if selected: content.extend(chunk)
            if output is not None: output.write(chunk)
        if count != info.file_size: raise InputError('model wheel member is truncated')
        if path.endswith('.so') or header[:4] == b'\x7fELF':
          if (len(header) < 64 or header[:7] != b'\x7fELF\x02\x01\x01'
              or int.from_bytes(header[18:20], 'little') != 183
              or int.from_bytes(header[20:24], 'little') != 1
              or int.from_bytes(header[52:54], 'little') != 64):
            raise InputError('model wheel native code is not AArch64 ELF')
        if output is not None:
          os.fchmod(output.fileno(), 0o444)
          output.flush()
          os.fsync(output.fileno())
      finally:
        if output is not None: output.close()
      hashes[path] = (checksum.hexdigest(), count)
      add(entries, target, dict(kind='file', sha256=checksum.hexdigest(), bytes=count, mode=0o444))
      if selected: metadata[path] = bytes(content)
      if count and any(label in PurePosixPath(path).name.lower() for label in ('license', 'copying', 'notice')):
        notices += 1
  if set(metadata) != {dist+'/METADATA', dist+'/WHEEL', dist+'/RECORD'} or notices == 0:
    raise InputError('model wheel lacks required metadata or notices')
  parsed = email.parser.BytesParser().parsebytes(metadata[dist+'/METADATA'])
  if parsed.get_all('Name') != [name] or parsed.get_all('Version') != [p['version']]:
    raise InputError('model wheel metadata differs from its package pin')
  for requirement in parsed.get_all('Requires-Dist', []):
    if not (name == 'zstandard' and requirement.startswith('cffi') and ' or ' not in requirement
            and re.search(r'\bextra\s*==\s*([\'"])cffi\1', requirement)):
      raise InputError('model wheel has an undeclared required dependency')
  wheel = email.parser.BytesParser().parsebytes(metadata[dist+'/WHEEL'])
  tags = {f'cp312-cp312-{tag}' for tag in p['filename'].removesuffix('.whl').split('-', 4)[4].split('.')}
  if wheel.get_all('Root-Is-Purelib') != ['false'] or set(wheel.get_all('Tag', [])) != tags:
    raise InputError('model wheel ABI/platform tags differ from the filename')
  recorded = set()
  for row in csv.reader(io.StringIO(metadata[dist+'/RECORD'].decode())):
    if len(row) != 3 or row[0] in recorded or row[0] not in hashes:
      raise InputError('model wheel RECORD is incomplete, duplicated or unsafe')
    path, expected, size = row
    recorded.add(path)
    if path == dist+'/RECORD':
      if expected or size: raise InputError('model wheel RECORD must not hash itself')
    else:
      digest_value, length = hashes[path]
      value = 'sha256='+base64.urlsafe_b64encode(bytes.fromhex(digest_value)).rstrip(b'=').decode()
      if expected != value or size != str(length): raise InputError('model wheel RECORD differs from actual bytes')
  if recorded != set(hashes): raise InputError('model wheel RECORD omits an actual file')
  provenance = encoded(package_identity(name, p))
  path = f'provenance/{name}.json'
  add(entries, path, dict(kind='file', sha256=sha(provenance), bytes=len(provenance), mode=0o444))
  if destination is not None: write(destination/path, provenance)
  tree = dict(schema=1, entries=entries)
  if len(encoded(tree)) > 16*1024**2: raise InputError('model wheel tree receipt exceeds 16 MiB')
  return tree


def verify(tree, root):
  if not stat.S_ISDIR(root.lstat().st_mode): raise InputError('model prefix is redirected')
  actual, pending = set(), [Path()]
  while pending:
    parent = pending.pop()
    for child in (root/parent).iterdir():
      path = (parent/child.name).as_posix()
      actual.add(path)
      expected = tree['entries'].get(path)
      if expected is None: raise InputError('model prefix has an unexpected member')
      mode = child.lstat().st_mode
      if expected['kind'] == 'directory':
        if not stat.S_ISDIR(mode): raise InputError('model prefix directory is redirected')
        pending.append(parent/child.name)
      elif (not stat.S_ISREG(mode) or stat.S_IMODE(mode) != expected['mode']
            or child.lstat().st_size != expected['bytes'] or digest(child, expected['bytes']) != expected['sha256']):
        raise InputError('model prefix differs from independently authenticated wheel contents')
  if actual != set(tree['entries']): raise InputError('model prefix omits a required member')


def sync_directories(root):
  for parent, _, _ in os.walk(root, topdown=False):
    fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try: os.fsync(fd)
    finally: os.close(fd)


def publish(temporary, destination):
  libc = ctypes.CDLL(None, use_errno=True)
  rename = libc.renameat2
  rename.argtypes = (ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint)
  rename.restype = ctypes.c_int
  if rename(-100, os.fsencode(temporary), -100, os.fsencode(destination), 1):
    error = ctypes.get_errno()
    raise OSError(error, os.strerror(error))
  fd = os.open(destination.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
  try: os.fsync(fd)
  finally: os.close(fd)


@contextmanager
def stage(store, identity, tree, growth):
  directory(store)
  parent = store/identity
  directory(parent)
  lock = os.open(parent/'lock', os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
  try:
    if not stat.S_ISREG(os.fstat(lock).st_mode): raise InputError('model prefix lock is not regular')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    generation = parent/'generation'
    tree_bytes = encoded(tree)
    receipt = dict(schema=1, identity=identity, tree_sha256=sha(tree_bytes))
    if generation.exists() or generation.is_symlink():
      if not stat.S_ISDIR(generation.lstat().st_mode): raise InputError('model generation is redirected')
      if read_json(generation/'receipt.json', 65536) != receipt or read_bytes(generation/'tree.json', len(tree_bytes)) != tree_bytes:
        raise InputError('model prefix receipt differs from authenticated inputs')
      verify(tree, generation/'tree')
      yield generation/'tree', None
      return
    if shutil.disk_usage(parent).free < growth: raise InputError('insufficient capacity for this model input operation; prior prefixes retained')
    with tempfile.TemporaryDirectory(prefix='.pending-', dir=parent) as temporary:
      root = Path(temporary)/'tree'
      root.mkdir()
      yield root, root
      verify(tree, root)
      write(Path(temporary)/'tree.json', tree_bytes)
      write(Path(temporary)/'receipt.json', encoded(receipt))
      sync_directories(Path(temporary))
      publish(Path(temporary), generation)
  finally:
    os.close(lock)


def prepare(plan, receipts, store, environment_store):
  validate(plan)
  if set(receipts) != {'model-wheel-'+name for name in plan['packages']}:
    raise InputError('model wheel receipt roster differs from its complete plan')
  packages = {}
  for name, p in sorted(plan['packages'].items()):
    raw = receipts['model-wheel-'+name]
    if raw['sha256'] != p['asset']['sha256'] or raw['bytes'] != p['asset']['bytes']:
      raise InputError('model wheel raw receipt differs from the declared asset')
    tree = wheel_tree(name, p, Path(raw['path']))
    identity = sha(encoded(package_identity(name, p)))
    with stage(store, identity, tree, p['maximum_unpacked_bytes']+METADATA_GROWTH) as (_, destination):
      if destination is not None:
        if wheel_tree(name, p, Path(raw['path']), destination) != tree:
          raise InputError('model wheel changed during preparation')
    root = store/identity/'generation/tree'
    packages[name] = dict(identity=identity, root=str(root), tree_receipt=str(root.parent/'tree.json'),
        tree_sha256=sha(encoded(tree)), tree=tree,
        copied_bytes=sum(e.get('bytes', 0) for e in tree['entries'].values()) if destination is not None else 0)
  entries, origins = {}, {}
  for p in packages.values():
    for path, entry in p['tree']['entries'].items():
      if path in entries and entry['kind'] != 'directory': raise InputError('model wheel packages overlap a file')
      add(entries, path, entry)
      if entry['kind'] == 'file': origins[path] = Path(p['root'])/path
  identity = sha(encoded(dict(schema=1, role='openpilot-model-wheels-v1', packages={n:p['identity'] for n,p in packages.items()})))
  tree = dict(schema=1, entries=entries)
  linked = 0
  with stage(environment_store, identity, tree, METADATA_GROWTH) as (_, destination):
    if destination is not None:
      if store.stat().st_dev != environment_store.stat().st_dev:
        raise InputError('model wheel prefixes and their composition require one filesystem')
      for path, entry in sorted(entries.items()):
        target = destination/path
        if entry['kind'] == 'directory': target.mkdir(parents=True, exist_ok=True)
        else:
          target.parent.mkdir(parents=True, exist_ok=True)
          os.link(origins[path], target, follow_symlinks=False)
          linked += entry['bytes']
  root = environment_store/identity/'generation/tree'
  return dict(schema=1, identity=identity, root=str(root), site_packages=str(root/'site-packages'),
      tree_receipt=str(root.parent/'tree.json'), tree_sha256=sha(encoded(tree)),
      packages={n:{k:v for k,v in p.items() if k!='tree'} for n,p in packages.items()},
      copied_bytes=sum(p['copied_bytes'] for p in packages.values()), linked_bytes=linked,
      installer_executed=False, wheels_imported=False, compiler_executed=False, model_generated=False,
      runtime_activated=False)


def source_identity(root):
  def git(*args):
    return subprocess.check_output(['/usr/bin/git', '-C', root, *args], text=True).strip()
  if Path(git('rev-parse', '--show-toplevel')) != root or git('status', '--porcelain', '--untracked-files=all', '--ignore-submodules=none'):
    raise InputError('model preparation source is not a clean repository root')
  return dict(commit=git('rev-parse', 'HEAD'), tree=git('rev-parse', 'HEAD^{tree}'))


def outside(root, path):
  if not path.is_absolute() or '..' in path.parts or path.is_symlink(): raise InputError('model stores require absolute real paths')
  path = path.parent.resolve(strict=True)/path.name
  if path.is_relative_to(root) or root.is_relative_to(path): raise InputError('model input stores must be outside the source')
  if path.exists() and not stat.S_ISDIR(path.lstat().st_mode): raise InputError('model store is not a directory')
  return path


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--preparer', type=Path, required=True)
  parser.add_argument('--cache', type=Path, required=True)
  parser.add_argument('--store', type=Path, required=True)
  parser.add_argument('--environment-store', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  started = time.monotonic()
  root = args.root.resolve(strict=True)
  source = source_identity(root)
  plan = read_json(root/'rust/native-model-wheels.json')
  validate(plan)
  paths = [outside(root, path) for path in (args.cache, args.store, args.environment_store)]
  for i, path in enumerate(paths):
    for other in paths[i+1:]:
      if path.is_relative_to(other) or other.is_relative_to(path): raise InputError('model input stores overlap')
  cache, store, environment = paths
  output = args.output.parent.resolve(strict=True)/args.output.name
  if output.is_relative_to(root) or any(output.is_relative_to(p) or p.is_relative_to(output) for p in paths):
    raise InputError('model preparation report overlaps source or immutable inputs')
  assets = dict(schema=1, reserve_bytes=sum(p['maximum_unpacked_bytes'] for p in plan['packages'].values())+METADATA_GROWTH,
                assets={'model-wheel-'+name:p['asset'] for name,p in plan['packages'].items()})
  with tempfile.TemporaryDirectory(prefix='model-wheel-plan-', dir=output.parent) as temporary:
    path = Path(temporary)/'assets.json'
    path.write_bytes(encoded(assets))
    raw = Path(temporary)/'receipts.json'
    with raw.open('xb') as stream:
      subprocess.run([args.preparer, '--prepare-assets', '--plan', path, '--cache', cache], stdout=stream, check=True)
    receipts = read_json(raw)
    result = prepare(plan, receipts, store, environment)
  if source_identity(root) != source: raise InputError('source changed during model wheel preparation')
  result.update(source=source, plan_sha256=digest(root/'rust/native-model-wheels.json'),
      preparer_sha256=digest(args.preparer), assets=receipts, elapsed_ms=int(1000*(time.monotonic()-started)),
      transferred_bytes=sum(row['transferred_bytes'] for row in receipts.values()),
      stored_bytes=sum(row['stored_bytes'] for row in receipts.values()))
  with output.open('xb') as stream:
    stream.write(encoded(result))
    stream.flush()
    os.fsync(stream.fileno())
  print(json.dumps({k:v for k,v in result.items() if k not in ('packages','assets')}))


if __name__ == '__main__':
  main()

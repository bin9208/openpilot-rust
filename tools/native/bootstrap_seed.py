#!/usr/bin/env python3
"""Pin, prepare and verify a small native inputs tool, independently of a runtime.

Python's standard library breaks the first-tool bootstrap cycle. No runtime is
installed or selected here. A published seed requires separately completed gates.
"""
import argparse
from contextlib import contextmanager
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile

ROLE = 'openpilot-runtime-inputs'
FILES = {'helper': 'openpilot-runtime-inputs-aarch64', 'notices': 'THIRD_PARTY_NOTICES.native-bootstrap.txt'}
AGNOS = '19.8-carrot-bt1'
ENV = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}


@contextmanager
def regular(path):
  fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
  with os.fdopen(fd, 'rb') as stream:
    if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
      raise ValueError('seed input is not a regular file')
    yield stream


def digest(path):
  with regular(path) as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def json_bytes(value):
  return (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()


def directory(path):
  path.mkdir(mode=0o700, exist_ok=True)
  if not stat.S_ISDIR(path.lstat().st_mode):
    raise ValueError('seed store requires real directories')
  return path.resolve(strict=True)


def write(path, data, mode=0o444):
  with path.open('xb') as stream:
    stream.write(data)
    os.fchmod(stream.fileno(), mode)
    stream.flush()
    os.fsync(stream.fileno())


def sync(path):
  fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
  try:
    os.fsync(fd)
  finally:
    os.close(fd)


def publish(source, destination):
  libc = ctypes.CDLL(None, use_errno=True)
  libc.renameat2.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
  libc.renameat2.restype = ctypes.c_int
  if libc.renameat2(-100, os.fsencode(source), -100, os.fsencode(destination), 1):
    raise OSError(ctypes.get_errno(), 'cannot publish a new seed generation')
  sync(destination.parent)


def elf(path):
  with regular(path) as stream:
    header = stream.read(64)
  if len(header) != 64 or header[:7] != b'\x7fELF\x02\x01\x01' or int.from_bytes(header[18:20], 'little') != 183 or int.from_bytes(header[20:24], 'little') != 1 or int.from_bytes(header[52:54], 'little') != 64:
    raise ValueError('seed is not an AArch64 ELF')


def file_pin(path):
  resolved = Path(path).resolve(strict=True)
  with regular(resolved) as stream:
    info = os.fstat(stream.fileno())
    return dict(resolved=str(resolved), bytes=info.st_size, mode=stat.S_IMODE(info.st_mode),
                sha256=hashlib.file_digest(stream, 'sha256').hexdigest())


def native():
  if platform.machine() != 'aarch64' or Path('/VERSION').read_text().strip() != AGNOS:
    raise ValueError('native seed execution requires the pinned AGNOS AArch64 userspace')


def inspect_abi(binary):
  """Inspect a trusted source-built seed and its actual default loader mapping."""
  def output(args):
    result = subprocess.run(args, env=ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30, check=True)
    if len(result.stdout) + len(result.stderr) > 1024**2:
      raise ValueError('seed ABI inspection exceeded its output bound')
    return result.stdout.decode()
  program = output(['/usr/bin/readelf', '--wide', '-l', str(binary)])
  match = re.search(r'Requesting program interpreter: ([^\]]+)\]', program)
  if not match or match[1] != '/lib/ld-linux-aarch64.so.1':
    raise ValueError('unexpected native seed interpreter')
  libraries = {}
  for line in output(['/usr/bin/ldd', str(binary)]).splitlines():
    fields = line.split()
    if not fields or fields[0] == 'linux-vdso.so.1':
      continue
    if len(fields) >= 3 and fields[1] == '=>' and fields[2].startswith('/'):
      path = fields[2]
    elif fields[0].startswith('/'):
      path = fields[0]
    else:
      raise ValueError('unresolved or unsupported native seed dependency')
    if not (path.startswith('/lib/') or path.startswith('/usr/lib/')) or path in libraries:
      raise ValueError('unexpected native seed provider path')
    libraries[path] = file_pin(path)
  if match[1] not in libraries:
    raise ValueError('native seed interpreter was not resolved')
  return dict(interpreter=match[1], libraries=libraries,
              tools={path: file_pin(path) for path in ('/usr/bin/readelf', '/usr/bin/ldd')})


def read_plan(path, expected):
  with regular(path) as stream:
    data = stream.read(1024**2 + 1)
  if len(data) > 1024**2 or hashlib.sha256(data).hexdigest() != expected:
    raise ValueError('seed descriptor differs from its independently pinned SHA')
  plan = json.loads(data)
  if set(plan) != {'schema', 'role', 'source', 'build', 'target', 'agnos', 'assets', 'abi'} or plan['schema'] != 1 or plan['role'] != ROLE or plan['target'] != 'aarch64-unknown-linux-gnu' or plan['agnos'] != AGNOS:
    raise ValueError('unsupported seed descriptor')
  if set(plan['source']) != {'commit', 'tree'} or any(not re.fullmatch('[0-9a-f]{40}', value) for value in plan['source'].values()):
    raise ValueError('seed source identity is incomplete')
  hashes = {'cargo_lock_sha256','cargo_inputs_plan_sha256','cargo_inputs_tree_sha256','rust_inputs_plan_sha256','rustc_sha256','cargo_sha256'}
  if set(plan['build']) != hashes | {'features'} or any(not re.fullmatch('[0-9a-f]{64}',plan['build'][key]) for key in hashes) or not isinstance(plan['build']['features'],list) or not {'prepare-rust','prepare-cargo','prepare-capnp'} <= set(plan['build']['features']):
    raise ValueError('seed build-input provenance is incomplete')
  if set(plan['assets']) != set(FILES) or set(plan['abi']) != {'interpreter', 'libraries', 'tools'} or plan['abi']['interpreter'] != '/lib/ld-linux-aarch64.so.1' or set(plan['abi']['tools']) != {'/usr/bin/ldd', '/usr/bin/readelf'} or not 1 <= len(plan['abi']['libraries']) <= 64:
    raise ValueError('seed asset or ABI inventory is incomplete')
  base = 'https://github.com/bin9208/openpilot-rust/releases/download/native-bootstrap-' + plan['source']['commit'] + '/'
  for name, asset in plan['assets'].items():
    limit = (64 if name == 'helper' else 16) * 1024**2
    if set(asset) != {'url', 'bytes', 'sha256'} or asset['url'] != base + FILES[name] or type(asset['bytes']) is not int or not 0 < asset['bytes'] <= limit or not re.fullmatch('[0-9a-f]{64}', asset['sha256']):
      raise ValueError('invalid pinned seed asset')
  for path, pin in (plan['abi']['libraries'] | plan['abi']['tools']).items():
    if not Path(path).is_absolute() or '..' in Path(path).parts or set(pin) != {'resolved', 'bytes', 'mode', 'sha256'} or not Path(pin['resolved']).is_absolute() or '..' in Path(pin['resolved']).parts or type(pin['bytes']) is not int or not 0 < pin['bytes'] <= 256 * 1024**2 or type(pin['mode']) is not int or pin['mode'] & 0o6000 or not re.fullmatch('[0-9a-f]{64}', pin['sha256']):
      raise ValueError('invalid native seed ABI pin')
  return plan, data


def fetch(asset, path):
  result = subprocess.run(['/usr/bin/curl', '--disable', '--fail', '--silent', '--show-error', '--location',
      '--proto', '=https', '--proto-redir', '=https', '--connect-timeout', '15', '--max-time', '1800',
      '--retry', '5', '--retry-all-errors', '--retry-max-time', '1800',
      '--max-filesize', str(asset['bytes']), '--write-out', '%{size_download}', '--output', str(path),
      '--url', asset['url']], env=ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
  return int(result.stdout)


def verify_generation(root, plan, data):
  if not stat.S_ISDIR(root.lstat().st_mode) or {p.name for p in root.iterdir()} != {'helper', 'notices', 'manifest.json'}:
    raise ValueError('seed generation inventory differs')
  for name, asset in plan['assets'].items():
    with regular(root/name) as stream:
      info = os.fstat(stream.fileno())
      if info.st_size != asset['bytes'] or stat.S_IMODE(info.st_mode) != (0o555 if name == 'helper' else 0o444) or hashlib.file_digest(stream, 'sha256').hexdigest() != asset['sha256']:
        raise ValueError('cached seed asset differs from its pin')
  with regular(root/'manifest.json') as stream:
    if stream.read(len(data) + 1) != data or stat.S_IMODE(os.fstat(stream.fileno()).st_mode) != 0o444:
      raise ValueError('cached seed descriptor differs')
  elf(root/'helper')


def prepare(plan_path, expected, cache, offline=None, download=fetch):
  plan, data = read_plan(plan_path, expected)
  cache = directory(cache)
  object_dir = directory(cache/expected)
  fd = os.open(object_dir/'lock', os.O_CREAT | os.O_WRONLY | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
  with os.fdopen(fd, 'wb') as lock:
    if not stat.S_ISREG(os.fstat(lock.fileno()).st_mode):
      raise ValueError('seed preparation lock is not regular')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    # Only this descriptor's locked, owned staging namespace is recoverable.
    # Published generations and other descriptors are never removed here.
    for pending in object_dir.iterdir():
      info = pending.lstat()
      if pending.name.startswith('.pending-') and stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid():
        shutil.rmtree(pending)
    root = object_dir/'generation'
    transferred = {name: 0 for name in FILES}
    stored = 0
    if not os.path.lexists(root):
      growth = sum(a['bytes'] for a in plan['assets'].values()) + len(data) + 1024**2
      if shutil.disk_usage(object_dir).free < growth:
        raise ValueError('insufficient space for this seed preparation; existing inputs retained')
      with tempfile.TemporaryDirectory(prefix='.pending-', dir=object_dir) as temporary:
        pending = Path(temporary)
        for name, asset in plan['assets'].items():
          destination = pending/name
          if offline is None:
            transferred[name] = download(asset, destination)
            if transferred[name] != asset['bytes']:
              raise ValueError('seed HTTP body count differs from the pinned asset size')
          else:
            with regular(offline/FILES[name]) as source, destination.open('xb') as output:
              copied = 0
              while block := source.read(65536):
                copied += len(block)
                if copied > asset['bytes']:
                  raise ValueError('offline seed exceeds its size pin')
                output.write(block)
          with regular(destination) as stream:
            if os.fstat(stream.fileno()).st_size != asset['bytes'] or hashlib.file_digest(stream, 'sha256').hexdigest() != asset['sha256']:
              raise ValueError('new seed asset differs from its pin')
            os.fchmod(stream.fileno(), 0o555 if name == 'helper' else 0o444)
            os.fsync(stream.fileno())
          stored += asset['bytes']
        write(pending/'manifest.json', data)
        stored += len(data)
        verify_generation(pending, plan, data)
        sync(pending)
        publish(pending, root)
        sync(cache)
    verify_generation(root, plan, data)
  return dict(schema=1, seed_source=plan['source'], helper=str(root/'helper'), root=str(root),
              descriptor_sha256=expected, transferred_bytes=transferred, stored_bytes=stored,
              seed_executed=False, compiler_executed=False, runtime_activated=False), plan


def verify_native(root, plan):
  native()
  for path, expected in (plan['abi']['libraries'] | plan['abi']['tools']).items():
    if file_pin(path) != expected:
      raise ValueError('native seed ABI provider or inspection tool differs from its pin')
  if inspect_abi(root/'helper') != plan['abi']:
    raise ValueError('native seed default loader mapping differs from its pin')


def current_tools(prepared, plan, configuration, source):
  """Execute only the pinned seed's fixed privilege-dropping bootstrap protocol."""
  verify_native(Path(prepared['root']), plan)
  prepared = {**prepared, 'native_abi_checked': True}
  if os.geteuid() == 0:
    raise ValueError('bootstrap caller must be the original non-root user')
  fields = Path('/proc/self/stat').read_text().rsplit(') ', 1)[1].split()
  owner = f'{os.getpid()}:{int(fields[19])}'
  configuration = configuration.resolve(strict=True)
  pin = digest(configuration)
  output = subprocess.check_output(['/usr/bin/sudo', '-n', prepared['helper'], '--bootstrap-tools-scope',
      '--config', str(configuration), '--configuration-sha256', pin, '--root', str(source.resolve(strict=True)),
      '--request-owner', owner], env=ENV)
  if len(output) > 4*1024**2:
    raise ValueError('bootstrap current-tools report exceeds 4 MiB')
  result = json.loads(output)
  if result.get('schema') != 1 or result.get('configuration_sha256') != pin or result.get('seed_helper_sha256') != plan['assets']['helper']['sha256'] or result.get('runtime_activated') is not False or result.get('runtime_started') is not False or result.get('runtime_release_downloaded') is not False:
    raise ValueError('bootstrap result differs from its pinned seed or preparation-only protocol')
  return dict(seed=prepared, current_tools=result, seed_native_abi_checked=True, seed_executed_for_current_tools=True,
              runtime_activated=False, runtime_started=False)


def notices(source, cargo, vendor, rust_licenses):
  metadata = json.loads(subprocess.check_output([str(cargo), 'metadata', '--locked', '--offline',
      '--manifest-path', str(source/'rust/Cargo.toml'), '--format-version', '1', '--filter-platform', 'aarch64-unknown-linux-gnu'], timeout=120))
  packages = {p['id']: p for p in metadata['packages']}
  nodes = {n['id']: n for n in metadata['resolve']['nodes']}
  roots = [p['id'] for p in metadata['packages'] if p['name'] == 'openpilot-runtime-build' and Path(p['manifest_path']).resolve() == source/'rust/crates/runtime-build/Cargo.toml']
  if len(roots) != 1:
    raise ValueError('source-built seed dependency root is missing')
  pending, seen = roots[:], set()
  while pending:
    key = pending.pop()
    if key in seen:
      continue
    seen.add(key)
    pending.extend(d['pkg'] for d in nodes[key]['deps'] if any(k['kind'] in (None, 'build') for k in d['dep_kinds']))
  output = bytearray(b'Native bootstrap inputs tool; project and dependency license texts.\n')
  def append_license(label, path):
    with regular(path) as stream:
      text = stream.read(1024**2 + 1)
    if len(text) > 1024**2:
      raise ValueError('seed license text exceeds 1 MiB')
    output.extend(f'\n\n--- {label} ---\n'.encode())
    output.extend(text)
    if len(output) > 16 * 1024**2:
      raise ValueError('seed license inventory exceeds 16 MiB')
  append_license('openpilot source LICENSE',source/'LICENSE')
  rust_files = sorted(p for p in rust_licenses.rglob('*') if p.is_file() and p.name in ('LICENSE-MIT','LICENSE-APACHE','COPYRIGHT'))
  if not {'LICENSE-MIT','LICENSE-APACHE','COPYRIGHT'} <= {p.name for p in rust_files}:
    raise ValueError('verified Rust distribution license inventory is missing')
  for path in rust_files:
    append_license('Rust distribution '+str(path.relative_to(rust_licenses)),path)
  for p in sorted((packages[k] for k in seen if packages[k]['source'] is not None), key=lambda p: (p['name'], p['version'])):
    root = Path(p['manifest_path']).resolve().parent
    if not root.is_relative_to(vendor):
      raise ValueError('seed dependency license is outside verified registry sources')
    files = sorted(path for path in root.rglob('*') if path.is_file() and any(path.name.upper().startswith(n) for n in ('LICENSE', 'COPYING', 'COPYRIGHT', 'NOTICE')))
    if not files:
      raise ValueError('seed dependency license text is missing: ' + p['name'])
    for path in files:
      append_license(f'{p["name"]} {p["version"]}: {path.relative_to(root)}',path)
  return bytes(output), sorted(nodes[roots[0]]['features'])


def export(source, binary, cargo, vendor, rust_licenses, output):
  native()
  source = source.resolve(strict=True)
  def git(*args):
    return subprocess.check_output(['/usr/bin/git', '-C', str(source), *args], text=True).strip()
  def identity():
    if git('rev-parse', '--show-toplevel') != str(source) or git('status', '--porcelain', '--untracked-files=all', '--ignore-submodules=none'):
      raise ValueError('seed export requires exact clean source')
    return dict(commit=git('rev-parse', 'HEAD'), tree=git('rev-parse', 'HEAD^{tree}'))
  before = identity()
  parent = output.parent.resolve(strict=True)
  if parent.is_relative_to(source):
    raise ValueError('seed export must be outside the source checkout')
  elf(binary)
  abi = inspect_abi(binary)
  checksum = digest(binary)
  vendor = vendor.resolve(strict=True)
  text, features = notices(source, cargo, vendor, rust_licenses.resolve(strict=True))
  build = dict(cargo_lock_sha256=digest(source/'rust/Cargo.lock'),
      cargo_inputs_plan_sha256=digest(source/'rust/native-cargo-inputs.json'),
      cargo_inputs_tree_sha256=digest(vendor.parent.parent/'tree.json'),
      rust_inputs_plan_sha256=digest(source/'rust/native-rust-toolchain.json'),
      rustc_sha256=digest(cargo.parent/'rustc'),cargo_sha256=digest(cargo),features=features)
  with tempfile.TemporaryDirectory(prefix='.seed-export-', dir=parent) as temporary:
    root = Path(temporary)
    with regular(binary) as stream:
      content = stream.read(64 * 1024**2 + 1)
    if len(content) > 64 * 1024**2 or hashlib.sha256(content).hexdigest() != checksum:
      raise ValueError('seed executable changed or exceeds 64 MiB')
    write(root/FILES['helper'], content, 0o555)
    write(root/FILES['notices'], text)
    base = 'https://github.com/bin9208/openpilot-rust/releases/download/native-bootstrap-' + before['commit'] + '/'
    assets = {name:dict(url=base+filename, sha256=digest(root/filename), bytes=(root/filename).stat().st_size) for name, filename in FILES.items()}
    plan = dict(schema=1, role=ROLE, source=before, build=build, target='aarch64-unknown-linux-gnu', agnos=AGNOS, assets=assets, abi=abi)
    data = json_bytes(plan)
    write(root/'manifest.json', data)
    checksum_plan = hashlib.sha256(data).hexdigest()
    write(root/'manifest.sha256', (checksum_plan+'\n').encode())
    read_plan(root/'manifest.json', checksum_plan)
    if identity() != before or digest(binary) != checksum or inspect_abi(binary) != abi:
      raise ValueError('seed source, executable or ABI changed during export')
    sync(root)
    publish(root, parent/output.name)
  return dict(source=before, descriptor_sha256=checksum_plan, output=str(parent/output.name), published_release=False, runtime_activated=False)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  commands = parser.add_subparsers(dest='command', required=True)
  create = commands.add_parser('export')
  for flag in ('source', 'binary', 'cargo', 'vendor', 'rust-licenses', 'output'):
    create.add_argument('--'+flag, type=Path, required=True)
  for name in ('prepare', 'build-tools'):
    use = commands.add_parser(name)
    use.add_argument('--manifest', type=Path, required=True)
    use.add_argument('--manifest-sha256', required=True)
    use.add_argument('--cache', type=Path, required=True)
    use.add_argument('--offline-assets', type=Path)
    use.add_argument('--verify-native', action='store_true')
    if name == 'build-tools':
      use.add_argument('--config', type=Path, required=True)
      use.add_argument('--source', type=Path, required=True)
  args = parser.parse_args()
  if args.command == 'export':
    result = export(args.source, args.binary, args.cargo, args.vendor, args.rust_licenses, args.output)
  else:
    result, plan = prepare(args.manifest, args.manifest_sha256, args.cache, args.offline_assets)
    if args.verify_native:
      verify_native(Path(result['root']), plan)
      result['native_abi_checked'] = True
    else:
      result['native_abi_checked'] = False
    if args.command == 'build-tools':
      result = current_tools(result, plan, args.config, args.source)
  print(json.dumps(result, indent=2))


if __name__ == '__main__':
  try:
    main()
  except (ValueError, TypeError, KeyError, OSError, subprocess.SubprocessError) as error:
    print('native bootstrap seed: ' + str(error), file=sys.stderr)
    sys.exit(1)

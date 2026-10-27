"""Compose the complete native runtime data set, retaining independent caches.

Python remains a build-time input preparer. No project runtime executable,
compiler/header tree, mutable model cache or old runtime manifest is installed.
"""
from pathlib import Path, PurePosixPath
import os
import stat
import subprocess
import sys
import tempfile

if __name__ == '__main__':
  if not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
    raise RuntimeError('runtime layout preparation requires Python -I -B -S')
  bytecode = Path(sys._xoptions.get('pycache_prefix', ''))
  if not bytecode.is_absolute():
    raise RuntimeError('runtime layout preparation requires a private absolute pycache_prefix')
  bytecode.mkdir(exist_ok=True)
  if any(bytecode.iterdir()):
    raise RuntimeError('runtime layout bytecode prefix must be empty')
  sys.path.insert(0, str(Path(__file__).resolve().parent))

from native_model_inputs import InputError, add, digest, encoded, normal, read_bytes, read_json, sha, stage, write

PROFILES = ('ffmpeg', 'raylib', 'opencv', 'numerics', 'geos', 'brotli', 'acados',
            'sdl', 'turbojpeg', 'ffmpeg-tools', 'freesans', 'panda', 'mpc', 'dbc', 'usbgpu')
ROLES = {*PROFILES, 'models', 'source-lfs', 'usbgpu-model'}
CONTRACTS = {'panda': 'rust/bin/licenses/panda/source-contract.json',
             'mpc': 'plannerd-acados/source-contract.json',
             'dbc': 'rust/bin/radarcan-dbc/source-contract.json',
             'usbgpu': 'rust/bin/licenses/usbgpu/source-contract.json'}
ORDINARY = (
  'openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf',
  'openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png',
  'openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png',
  'openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png',
  'openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png',
  'openpilot/selfdrive/assets/images/speed_bg.png', 'openpilot/selfdrive/assets/images/traffic_green.png',
  'openpilot/selfdrive/assets/images/traffic_red.png',
  'openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png',
)


def git(root, *args, limit=64*1024**2):
  result = subprocess.run(['/usr/bin/git', '-C', str(root), *args], stdout=subprocess.PIPE,
                          check=True, env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0'})
  if len(result.stdout) > limit:
    raise InputError('runtime layout Git input exceeds its bound')
  return result.stdout


def source(root):
  if git(root, 'status', '--porcelain', '--untracked-files=all').strip():
    raise InputError('runtime layout requires a clean source checkout')
  return dict(commit=git(root, 'rev-parse', 'HEAD').decode().strip(),
              tree=git(root, 'rev-parse', 'HEAD^{tree}').decode().strip())


def input_tree(pin):
  if set(pin) != {'root', 'receipt', 'sha256'}:
    raise InputError('runtime role requires its complete independently pinned tree')
  root, receipt = Path(pin['root']), Path(pin['receipt'])
  if not root.is_absolute() or not receipt.is_absolute() or not stat.S_ISDIR(root.lstat().st_mode):
    raise InputError('runtime role paths must be absolute and its root a real directory')
  raw = read_bytes(receipt, 16*1024**2)
  if sha(raw) != pin['sha256']:
    raise InputError('runtime role receipt differs from its independent SHA')
  tree = read_json(receipt, 16*1024**2)
  if set(tree) != {'schema', 'entries'} or tree['schema'] != 1 or not 0 < len(tree['entries']) <= 100000:
    raise InputError('invalid runtime role tree')
  entries = tree['entries']
  pending, actual = [Path()], set()
  while pending:
    parent = pending.pop()
    for child in (root/parent).iterdir():
      name = (parent/child.name).as_posix()
      if not normal(name) or name not in entries:
        raise InputError('unexpected runtime role member')
      actual.add(name)
      entry, info = entries[name], child.lstat()
      if entry == {'kind': 'directory'}:
        if not stat.S_ISDIR(info.st_mode):
          raise InputError('runtime role directory is redirected')
        pending.append(parent/child.name)
      elif entry.get('kind') == 'file' and set(entry) == {'kind', 'sha256', 'bytes', 'mode'}:
        if (not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) != entry['mode']
            or info.st_size != entry['bytes'] or digest(child, entry['bytes']) != entry['sha256']):
          raise InputError('runtime role differs from its pinned tree')
      elif entry.get('kind') == 'symlink' and set(entry) == {'kind', 'target'}:
        target = Path(entry['target'])
        resolved = child.resolve(strict=True)
        if (not stat.S_ISLNK(info.st_mode) or target.is_absolute() or os.readlink(child) != str(target)
            or not resolved.is_relative_to(root) or not resolved.is_file()
            or entries.get(resolved.relative_to(root).as_posix(), {}).get('kind') != 'file'):
          raise InputError('runtime role alias escapes or lacks its pinned file')
      else:
        raise InputError('invalid runtime role entry')
  if actual != set(entries):
    raise InputError('runtime role omitted a pinned member')
  return root, entries


def compose(root, spec, store, verifier):
  if set(spec) != {'schema', 'roles', 'toolchain_root', 'libraries',
                   'expected_source_growth_bytes', 'remaining_object_growth_bytes'} or spec['schema'] != 1:
    raise InputError('invalid complete runtime layout specification')
  if set(spec['roles']) != ROLES:
    raise InputError('complete runtime layout requires every provider/model/data role')
  root = root.resolve(strict=True)
  store = store.absolute()
  if any(part in ('.', '..') for part in store.parts):
    raise InputError('runtime input store requires a contained absolute spelling')
  store = store.parent.resolve(strict=True)/store.name
  before = source(root)
  if store.is_relative_to(root) or root.is_relative_to(store) or store.is_symlink():
    raise InputError('runtime input store must be outside source')
  producers = {}
  for name in ('native_runtime_layout.py', 'native_model_inputs.py'):
    path = Path(__file__).resolve().parent/name
    raw = git(root, 'show', 'HEAD:rust/tools/'+name)
    if raw != read_bytes(path, 1024**2):
      raise InputError('executing layout producer differs from current source')
    producers[name] = sha(raw)
  trees = {role: input_tree(pin) for role, pin in spec['roles'].items() if role != 'usbgpu-model'}
  for prefix, _ in trees.values():
    if store.is_relative_to(prefix.parent) or prefix.parent.is_relative_to(store):
      raise InputError('composition store must be disjoint from every prepared input')
  entries, mapping, origins, metadata, owners = {}, {}, {}, {}, {}

  def put(target, entry, origin=None, data=None, role=None):
    if not normal(target) or not target.startswith(('rust/bin/', 'openpilot/', 'panda/board/obj/')):
      raise InputError('unsafe or unsupported runtime layout destination')
    relative = target.removeprefix('rust/bin/') if target.startswith('rust/bin/') else 'source-assets/'+target
    if entry['kind'] != 'file' or entry['mode'] not in (0o444, 0o555):
      raise InputError('runtime composition admits only immutable regular files')
    if PurePosixPath(target).parent == PurePosixPath('rust/bin') and target not in ('rust/bin/ffmpeg', 'rust/bin/ffprobe'):
      raise InputError('provider cannot supply a project runtime executable')
    if relative in entries:
      allowed = ({'acados', 'mpc'}, {'ffmpeg', 'ffmpeg-tools'}, {'sdl', 'freesans'})
      if entries[relative] != entry or {owners.get(relative), role} not in allowed:
        raise InputError('conflicting or unexpected overlap between runtime roles')
      return
    add(entries, relative, entry)
    mapping[target] = relative
    owners[relative] = role
    if data is not None:
      metadata[relative] = data
    else:
      origins[relative] = origin

  def member(role, name, target):
    prefix, tree = trees[role]
    entry = tree.get(name)
    if entry is None or entry['kind'] == 'directory':
      raise InputError('runtime profile is missing a pinned member')
    path = prefix/name
    if entry['kind'] == 'symlink':
      path = path.resolve(strict=True)
      entry = tree[path.relative_to(prefix).as_posix()]
    put(target, entry, origin=path, role=role)

  for role in PROFILES:
    plan = read_json(root/f'rust/native-{role}-provider.json')
    prefix, tree = trees[role]
    expected = {*plan['members'], 'provenance/native-provider.json'}
    if role in CONTRACTS:
      expected.add(CONTRACTS[role])
    if {name for name, e in tree.items() if e['kind'] != 'directory'} != expected:
      raise InputError('runtime provider has a narrowed or extended profile')
    provenance = read_json(prefix/'provenance/native-provider.json')
    kind = 'openpilot-source-bound-provider-v1' if role in CONTRACTS else 'openpilot-foreign-provider-v1'
    if provenance != [kind, plan]:
      raise InputError('runtime provider provenance differs from its current pinned profile')
    for name, row in plan['members'].items():
      path = prefix/name
      if row['link'] is not None:
        if tree[name] != dict(kind='symlink', target=row['link']):
          raise InputError('runtime provider alias differs from its profile')
        path = path.resolve(strict=True)
      entry = tree[path.relative_to(prefix).as_posix()]
      if entry['sha256'] != row['sha256'] or (row['link'] is None
          and (entry['bytes'] != row['bytes'] or entry['mode'] != row['mode'])):
        raise InputError('runtime provider content differs from its source-pinned profile')
      target = row['source']
      if target == 'rust/runtime-manifest.json':
        target = f'rust/bin/licenses/native-inputs/{role}/original-runtime-manifest.json'
      if role == 'numerics' and target.startswith('rust/bin/card-numerics/'):
        for directory in ('card-numerics', 'controlsd-numerics', 'torqued-numerics', 'radarcan-numerics'):
          member(role, name, target.replace('rust/bin/card-numerics/', f'rust/bin/{directory}/', 1))
      else:
        member(role, name, target)
    member(role, 'provenance/native-provider.json', f'rust/bin/licenses/native-inputs/{role}/provider.json')
    if role in CONTRACTS:
      target = CONTRACTS[role]
      if not target.startswith('rust/bin/'):
        target = 'rust/bin/'+target
      member(role, CONTRACTS[role], target)

  for name, entry in trees['models'][1].items():
    if entry['kind'] != 'directory':
      if not name.startswith(('driving-models/', 'driver-models/', 'model-inputs/')):
        raise InputError('model runtime input contains non-runtime cache data')
      member('models', name, 'rust/bin/'+name)
  lfs = read_json(root/'rust/native-source-lfs.json')
  expected_lfs = {*lfs['assets'], 'provenance/committed-source-lfs.json'}
  if {n for n, e in trees['source-lfs'][1].items() if e['kind'] != 'directory'} != expected_lfs:
    raise InputError('source LFS input is incomplete or extended')
  for name in expected_lfs:
    target = 'rust/bin/licenses/native-inputs/source-lfs.json' if name.startswith('provenance/') else name
    member('source-lfs', name, target)

  model = read_json(root/'rust/native-usbgpu-model.json')['assets']['usbgpu-model']
  raw = spec['roles']['usbgpu-model']
  if set(raw) != {'path', 'sha256', 'bytes'} or raw['sha256'] != model['sha256'] or raw['bytes'] != model['bytes']:
    raise InputError('eGPU model differs from the separately pinned raw asset')
  path = Path(raw['path'])
  if not path.is_absolute() or stat.S_IMODE(path.lstat().st_mode) != 0o444 or digest(path, model['bytes']) != model['sha256']:
    raise InputError('eGPU raw model asset is damaged or mutable')
  put(f'rust/bin/usbgpu-assets/models/{model["sha256"]}/model.pkl',
      dict(kind='file', mode=0o444, bytes=model['bytes'], sha256=model['sha256']), origin=path, role='usbgpu-model')
  ordinary = {}
  for name in ORDINARY:
    data = git(root, 'show', 'HEAD:'+name)
    if not data or data.startswith(b'version https://git-lfs.github.com/spec/v1\n'):
      raise InputError('ordinary runtime asset is absent or now requires a new LFS input')
    ordinary[name] = sha(data)
    put(name, dict(kind='file', mode=0o444, bytes=len(data), sha256=sha(data)), data=data, role='ordinary-git')
  provenance = encoded(dict(schema=1, kind='openpilot-complete-native-inputs-v1', producers=producers,
      roles={r: p['sha256'] for r, p in spec['roles'].items()}, ordinary_git_assets=ordinary,
      payload_tree_sha256=sha(encoded(dict(schema=1, entries=entries)))))
  identity = sha(provenance)
  put('rust/bin/licenses/native-inputs/composition.json',
      dict(kind='file', mode=0o444, bytes=len(provenance), sha256=identity), data=provenance, role='composition')
  tree = dict(schema=1, entries=entries)
  tree_bytes = encoded(tree)
  growth = (sum(((len(b)+4095)//4096)*4096 for b in metadata.values())+2*len(tree_bytes)
            +sum(4096 if e['kind'] == 'directory' else 128+len(n.encode()) for n, e in entries.items())+65536)
  raw_parent = Path(raw['path']).parent
  if store.is_relative_to(raw_parent) or raw_parent.is_relative_to(store):
    raise InputError('composition store must be disjoint from the raw model cache')
  if any(path.stat().st_dev != store.parent.stat().st_dev for path in origins.values()):
    raise InputError('runtime input composition requires one filesystem for immutable links')
  store.mkdir(exist_ok=True)
  linked = copied = 0
  with stage(store, identity, tree, growth) as (prefix, destination):
    if destination is not None:
      for name, entry in sorted(entries.items()):
        target = destination/name
        if entry['kind'] == 'directory':
          target.mkdir(parents=True, exist_ok=True)
        elif name in metadata:
          write(target, metadata[name]); copied += len(metadata[name])
        else:
          os.link(origins[name], target, follow_symlinks=False); linked += entry['bytes']
    # Full current-source checks run against the prospective actual layout,
    # before atomic publication, including all models/Panda/MPC/DBC/eGPU/LFS.
    with tempfile.NamedTemporaryFile(dir=store, suffix='.json') as temporary:
      temporary.write(encoded(mapping)); temporary.flush()
      subprocess.run([str(verifier), '--verify-runtime-layout', '--root', str(root),
                      '--store', str(prefix), '--plan', temporary.name], check=True, stdout=subprocess.PIPE)
    for role, pin in spec['roles'].items():
      if role != 'usbgpu-model':
        input_tree(pin)
    if source(root) != before:
      raise InputError('runtime source changed during composition')
  prefix = store/identity/'generation/tree'
  receipt = prefix.parent/'tree.json'
  layout = {k: spec[k] for k in ('toolchain_root', 'libraries', 'expected_source_growth_bytes', 'remaining_object_growth_bytes')}
  layout.update(schema=1, provider_root=str(prefix), provider_receipt=str(receipt), files=mapping)
  return dict(schema=1, source=before, identity=identity, root=str(prefix), tree_receipt=str(receipt),
      tree_sha256=sha(tree_bytes), layout=layout, runtime_inputs=dict(files={str(receipt): sha(tree_bytes)},
      trees={str(prefix): str(receipt)}), copied_bytes=copied, linked_bytes=linked,
      source_data_verified=True, required_roles=len(ROLES), runtime_activated=False, full_runtime_build=False)


def main():
  import argparse
  parser = argparse.ArgumentParser()
  for name in ('root', 'spec', 'store', 'verifier', 'report'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  source_root = args.root.resolve(strict=True)
  output = args.report.parent.resolve(strict=True)/args.report.name
  store = args.store.parent.resolve(strict=True)/args.store.name
  if output.is_relative_to(source_root) or output.is_relative_to(store):
    raise InputError('runtime layout report must be outside source and immutable inputs')
  result = compose(args.root, read_json(args.spec, 16*1024**2), args.store, args.verifier)
  with args.report.open('xb') as output:
    output.write(encoded(result))


if __name__ == '__main__':
  main()

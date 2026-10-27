#!/usr/bin/env python3
"""Resolve current Git-pinned inputs and invoke the complete native builder.

Device-local settings contain storage/resource policy. Download pins come from
the requested source's committed input index. No live SDK capture or whole
runtime release is a fallback for an absent or invalid index.
"""
import sys

if __name__ == '__main__' and not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
  raise SystemExit('native source entry requires /usr/bin/python3 -I -B -S')

import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
from urllib.parse import urlsplit

_BYTECODE = None
if __name__ == '__main__':
  _BYTECODE = tempfile.TemporaryDirectory(prefix='openpilot-native-entry-bytecode-', dir='/tmp')
  sys.pycache_prefix = _BYTECODE.name
_SPEC = importlib.util.spec_from_file_location('openpilot_native_source_runtime', Path(__file__).with_name('source_runtime.py'))
runtime = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(runtime)
seed = runtime.seed

INDEX = 'rust/native-bootstrap-inputs.json'
ROLES = ('ffmpeg', 'raylib', 'opencv', 'numerics', 'geos', 'brotli', 'acados', 'sdl',
         'turbojpeg', 'ffmpeg-tools', 'freesans', 'panda', 'mpc', 'dbc', 'usbgpu')
MACHINE_FIELDS = {'schema', 'stores', 'cache', 'seed_cache', 'updater_staging', 'expected_source_growth_bytes',
    'expected_compile_growth_bytes', 'remaining_object_growth_bytes', 'compiler_growth_bytes', 'resources', 'budgets'}


def directory(path):
  if seed.directory(path) != path:
    raise ValueError('native input metadata directory is redirected')
  return path


def positive(value):
  return type(value) is int and 0 < value <= (1 << 64)-1


def validate_machine(machine, source):
  if (set(machine) not in (MACHINE_FIELDS, MACHINE_FIELDS | {'boot_seed'}) or type(machine['schema']) is not int
      or machine['schema'] != 1 or set(machine['stores']) != {'sources', 'objects', 'generations', 'params'}
      or not all(positive(machine[k]) for k in ('expected_source_growth_bytes', 'expected_compile_growth_bytes',
          'remaining_object_growth_bytes', 'compiler_growth_bytes'))):
    raise ValueError('native machine settings require explicit storage and growth policy')
  paths = [runtime.absolute(machine[k]) for k in ('cache', 'seed_cache')]
  paths += [runtime.absolute(p) for p in machine['stores'].values()]
  paths.append(runtime.absolute(machine['updater_staging']))
  for i, path in enumerate(paths):
    if (path.is_symlink() or path.parent.resolve(strict=True) != path.parent
        or (path.exists() and (not path.is_dir() or path.resolve(strict=True) != path))
        or path.is_relative_to(source) or (i != len(paths)-1 and source.is_relative_to(path))
        or any(path.is_relative_to(p) or p.is_relative_to(path) for p in paths[i+1:])):
      raise ValueError('native machine settings need separate real stores outside source')
  policy = machine['resources']
  if (set(policy) != {'schema', 'memory_cgroup_parent', 'maximum_memory_bytes', 'minimum_available_memory_bytes', 'filesystems'}
      or type(policy['schema']) is not int or policy['schema'] != 1
      or not positive(policy['maximum_memory_bytes']) or not positive(policy['minimum_available_memory_bytes'])
      or policy['maximum_memory_bytes']+policy['minimum_available_memory_bytes'] > (1 << 64)-1
      or not 1 <= len(policy['filesystems']) <= 16):
    raise ValueError('native machine needs an explicit memory/filesystem resource policy')
  runtime.absolute(policy['memory_cgroup_parent'])
  for name, budget in policy['filesystems'].items():
    runtime.absolute(name)
    if (set(budget) != {'maximum_growth_bytes', 'minimum_free_bytes'}
        or type(budget['maximum_growth_bytes']) is not int or budget['maximum_growth_bytes'] < 0
        or not positive(budget['minimum_free_bytes'])
        or budget['maximum_growth_bytes']+budget['minimum_free_bytes'] > (1 << 64)-1):
      raise ValueError('native machine filesystem growth/free budget is invalid')
  budgets = machine['budgets']
  if (set(budgets) != {'schema', 'group_growth_bytes', 'expected_source_growth_bytes', 'remaining_object_growth_bytes', 'estimate_basis'}
      or type(budgets['schema']) is not int or budgets['schema'] != 1
      or not budgets['group_growth_bytes'] or not all(positive(v) for v in budgets['group_growth_bytes'].values())
      or not positive(budgets['expected_source_growth_bytes']) or not positive(budgets['remaining_object_growth_bytes'])
      or not isinstance(budgets['estimate_basis'], str) or not budgets['estimate_basis'].strip()
      or len(budgets['estimate_basis']) > 4096):
    raise ValueError('native machine build estimates need complete groups and an explicit basis')
  if 'boot_seed' in machine:
    validate_asset(machine['boot_seed'], 1024**2)


def validate_asset(asset, maximum):
  if (set(asset) != {'url', 'sha256', 'bytes'} or not positive(asset['bytes']) or asset['bytes'] > maximum
      or not re.fullmatch('[0-9a-f]{64}', asset['sha256'])):
    raise ValueError('invalid independent native metadata asset pin')
  url = urlsplit(asset['url'])
  if (url.scheme != 'https' or url.netloc != 'github.com' or url.query or url.fragment
      or not url.path.startswith('/bin9208/openpilot-rust/releases/download/')
      or any(p in ('.', '..') for p in url.path.split('/'))):
    raise ValueError('native metadata must use its source-pinned repository release URL')


def git_json(source, name):
  base = ['/usr/bin/git', '-C', str(source)]
  oid = subprocess.check_output([*base, 'rev-parse', '--verify', 'HEAD:'+name], env=seed.ENV, text=True).strip()
  if not re.fullmatch('[0-9a-f]{40}', oid):
    raise ValueError('native Git input has no exact blob identity')
  size = int(subprocess.check_output([*base, 'cat-file', '-s', oid], env=seed.ENV, text=True))
  if size > 4*1024**2:
    raise ValueError('native Git input index exceeds 4 MiB')
  result = subprocess.run([*base, 'cat-file', 'blob', oid],
      env=seed.ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
  if len(result.stdout) != size:
    raise ValueError('native Git input changed while reading its exact blob')
  return json.loads(result.stdout, object_pairs_hook=runtime.pairs), hashlib.sha256(result.stdout).hexdigest()


def input_index(source):
  index, checksum = git_json(source, INDEX)
  if (set(index) != {'schema', 'seed', 'system_sdk', 'providers'} or type(index['schema']) is not int
      or index['schema'] != 1 or set(index['providers']) != set(ROLES)):
    raise ValueError('native input index requires the complete current-source descriptor selection')
  validate_asset(index['seed'], 1024**2)
  validate_asset(index['system_sdk'], 16*1024**2)
  for asset in index['providers'].values():
    validate_asset(asset, 1024**2)
  return index, checksum


class Metadata:
  def __init__(self, machine, commands, observe, offline=None):
    self.machine, self.commands, self.observe, self.offline = machine, commands, observe, offline
    self.root = directory(Path(machine['cache']))
    self.root = directory(self.root/'metadata-assets')
    self.objects = directory(self.root/'sha256')

  def admit(self, needed):
    device = self.root.stat().st_dev
    budgets = {(b['maximum_growth_bytes'], b['minimum_free_bytes']) for p, b in self.machine['resources']['filesystems'].items()
        if Path(p).stat().st_dev == device}
    if len(budgets) != 1:
      raise ValueError('metadata cache needs one unambiguous budget for its filesystem')
    growth, reserve = next(iter(budgets))
    if needed > growth or shutil.disk_usage(self.root).free < needed+reserve:
      raise ValueError('insufficient budget for this metadata preparation; other caches retained')

  def asset(self, asset):
    self.observe()
    parent = directory(self.objects/asset['sha256'])
    fd = os.open(parent/'lock', os.O_CREAT | os.O_WRONLY | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
      if not stat.S_ISREG(os.fstat(fd).st_mode):
        raise ValueError('native metadata lock is not regular')
      fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
      blob = parent/'blob'
      transferred = stored = offline = 0
      if not blob.exists():
        self.admit(asset['bytes']+65536)
        with tempfile.TemporaryDirectory(prefix='.pending-', dir=parent) as temporary:
          pending = Path(temporary)/'blob'
          if self.offline is None:
            transferred = seed.fetch(asset, pending, run=self.commands.run)
          else:
            origin = self.offline/(asset['sha256']+'.json')
            with seed.regular(origin) as source, pending.open('xb') as output:
              while chunk := source.read(65536):
                self.observe()
                offline += len(chunk)
                if offline > asset['bytes']:
                  raise ValueError('offline native metadata exceeds its pinned byte count')
                output.write(chunk)
          with seed.regular(pending) as stream:
            if os.fstat(stream.fileno()).st_size != asset['bytes'] or hashlib.file_digest(stream, 'sha256').hexdigest() != asset['sha256']:
              raise ValueError('native descriptor differs from its independent Git pin')
          pending.chmod(0o444)
          with seed.regular(pending) as stream:
            os.fsync(stream.fileno())
          self.observe()
          seed.publish(pending, blob)
          stored = asset['bytes']
      with seed.regular(blob) as stream:
        info = os.fstat(stream.fileno())
        if (info.st_size != asset['bytes'] or stat.S_IMODE(info.st_mode) != 0o444
            or hashlib.file_digest(stream, 'sha256').hexdigest() != asset['sha256']):
          raise ValueError('cached native metadata differs from its immutable input pin')
      self.observe()
      return dict(path=str(blob), sha256=asset['sha256'], bytes=asset['bytes'],
          transferred_bytes=transferred, stored_bytes=stored, offline_seed_bytes=offline)
    finally:
      os.close(fd)


def resolve(machine, source, output, commands, observe, offline=None):
  validate_machine(machine, source)
  generations = Path(machine['stores']['generations'])
  if (not output.is_absolute() or not output.is_dir() or output.resolve(strict=True) != output
      or not output.is_relative_to(generations) or output.is_relative_to(source)):
    raise ValueError('resolved input configuration requires an owned generation request directory')
  index, index_sha = input_index(source)
  catalog, _ = git_json(source, 'rust/native-runtime-catalog.json')
  if set(machine['budgets']['group_growth_bytes']) != set(catalog['groups']):
    raise ValueError('machine growth estimates do not cover the current source catalog groups')
  cache = Metadata(machine, commands, observe, offline)
  receipts = {name: cache.asset(asset) for name, asset in [('seed', index['seed']), ('system_sdk', index['system_sdk'])]}
  for role in ROLES:
    receipts[role] = cache.asset(index['providers'][role])
    descriptor = runtime.read(Path(receipts[role]['path']), receipts[role]['sha256'])
    profile, _ = git_json(source, f'rust/native-{role}-provider.json')
    if descriptor['profile'] != profile['profile'] or descriptor['provider_source_commit'] != profile['source_commit']:
      raise ValueError('provider descriptor differs from the requested source profile')
  with seed.regular(Path(receipts['system_sdk']['path'])) as stream:
    data = stream.read(16*1024**2+1)
  if len(data) != receipts['system_sdk']['bytes'] or hashlib.sha256(data).hexdigest() != receipts['system_sdk']['sha256']:
    raise ValueError('SDK metadata changed during profile validation')
  sdk = json.loads(data, object_pairs_hook=runtime.pairs)
  sdk_plan, _ = git_json(source, 'rust/native-system-sdk.json')
  if sdk['plan'] != sdk_plan:
    raise ValueError('SDK metadata profile differs from the requested source')
  def write(name, value):
    path = output/name
    seed.write(path, seed.json_bytes(value))
    return path, seed.digest(path)
  budgets, budgets_sha = write('budgets.json', machine['budgets'])
  selected = dict(schema=1, providers={r: {k: receipts[r][k] for k in ('path', 'sha256')} for r in ROLES},
      budgets=dict(path=str(budgets), sha256=budgets_sha), compiler_growth_bytes=machine['compiler_growth_bytes'])
  selection, selection_sha = write('selection.json', selected)
  configuration = {k: machine[k] for k in runtime.TOOLS_FIELDS if k not in ('system_sdk_receipt', 'system_sdk_sha256')}
  configuration.update(system_sdk_receipt=receipts['system_sdk']['path'], system_sdk_sha256=receipts['system_sdk']['sha256'],
      runtime_inputs=dict(path=str(selection), sha256=selection_sha))
  config, config_sha = write('bootstrap.json', configuration)
  observe()
  return dict(schema=1, index_sha256=index_sha, receipts=receipts, configuration=str(config), configuration_sha256=config_sha)


def build_requested(args, machine, source, pin):
  before = runtime.source_identity(source)
  owner = seed.owner_chain(args.request_owner)
  commands = runtime.OwnedCommands(owner, Path(machine['stores']['params']))
  def observe():
    commands.observe()
    runtime.read(args.machine_config, pin)
    if runtime.source_identity(source) != before:
      raise ValueError('native source changed while resolving its input index')
  observe()
  generations = directory(Path(machine['stores']['generations']))
  with runtime.operation(generations) as attempt:
    # Release the resolver's lock before entering the full driver's same lock.
    resolved = resolve(machine, source, attempt, commands, observe)
    seed.write(attempt/'resolved.json', seed.json_bytes(resolved))
  prepared, plan = seed.prepare(Path(resolved['receipts']['seed']['path']), resolved['receipts']['seed']['sha256'],
      Path(machine['seed_cache']), download=lambda asset, path: seed.fetch(asset, path, run=commands.run))
  observe()
  result = runtime.build_runtime(prepared, plan, Path(resolved['configuration']), resolved['configuration_sha256'],
      source, owner, args.running_config, args.running_configuration_sha256, args.previous,
      expected_source=before, observe=observe)
  result.update(input_index_sha256=resolved['index_sha256'], metadata=resolved['receipts'],
      machine_configuration_sha256=pin, resolver_evidence=str(attempt))
  seed.write(attempt/'result.json', seed.json_bytes(result))
  return result


def startup_record(machine):
  path = Path(machine['stores']['generations'])/'startup.json'
  # A damaged or redirected record is preserved and rejected. Only absence is
  # permission to build an initial runtime; never reset an existing selection.
  try:
    path.lstat()
  except FileNotFoundError:
    return None
  return validate_startup(machine, runtime.read(path))


def validate_startup(machine, record):
  if (set(record) != {'schema', 'configuration', 'configuration_sha256', 'initial_generation'}
      or type(record['schema']) is not int or record['schema'] != 1
      or not all(isinstance(record[k], str) and re.fullmatch('[0-9a-f]{64}', record[k])
          for k in ('configuration_sha256', 'initial_generation'))):
    raise ValueError('invalid retained native startup record; state preserved')
  expected = Path(machine['stores']['generations'])/'configurations'/(record['configuration_sha256']+'.json')
  if record['configuration'] != str(expected):
    raise ValueError('native startup configuration escaped its immutable store')
  with seed.regular(expected) as stream:
    if stat.S_IMODE(os.fstat(stream.fileno()).st_mode) != 0o444:
      raise ValueError('native startup configuration is mutable')
  if runtime.read(expected, record['configuration_sha256'])['stores'] != machine['stores']:
    raise ValueError('native startup configuration belongs to different machine stores')
  return record


def retain_startup(machine, result):
  parent = directory(Path(machine['stores']['generations']))
  record = dict(schema=1, configuration=result['launch_configuration'],
      configuration_sha256=result['launch_configuration_sha256'], initial_generation=result['generation_id'])
  validate_startup(machine, record)
  with tempfile.TemporaryDirectory(prefix='.pending-startup-', dir=parent) as temp:
    path = Path(temp)/'startup.json'
    seed.write(path, seed.json_bytes(record), 0o444)
    try:
      seed.publish(path, parent/'startup.json')
    except FileExistsError:
      pass  # A competing boot's complete record wins; it is revalidated below.
  return startup_record(machine)


def boot_seed(machine, commands, observe):
  if 'boot_seed' not in machine:
    raise ValueError('native launch needs an independently pinned boot seed in machine settings')
  receipt = Metadata(machine, commands, observe).asset(machine['boot_seed'])
  prepared, plan = seed.prepare(Path(receipt['path']), receipt['sha256'], Path(machine['seed_cache']),
      download=lambda asset, path: seed.fetch(asset, path, run=commands.run))
  if plan.get('protocols', {}).get('generation_boot') != 1:
    raise ValueError('pinned native seed does not implement generation boot')
  observe()
  seed.verify_native(Path(prepared['root']), plan)
  return prepared


def launch(args, machine, source, pin):
  owner = seed.owner_chain(args.request_owner)
  commands = runtime.OwnedCommands(owner, Path(machine['stores']['params']))
  def observe():
    # A verified existing runtime must boot even if a stale IsOnroad survived a
    # power loss. Downloads/builds still use the offroad-only command driver.
    seed.verify_owners(owner)
    runtime.read(args.machine_config, pin)
  observe()
  prepared = boot_seed(machine, commands, observe)
  record = startup_record(machine)
  if record is None:
    result = build_requested(args, machine, source, pin)
    observe()
    record = retain_startup(machine, result)
    print(json.dumps(result, indent=2), file=sys.stderr)
  observe()
  argv = [prepared['helper'], '--boot-runtime', '--config', record['configuration'],
      '--configuration-sha256', record['configuration_sha256'], '--root', str(source),
      '--initial-generation', record['initial_generation']]
  if args.paths is not None:
    argv += ['--paths', str(args.paths)]
  if args.verify_package:
    argv += ['--verify-package']
  env = dict(os.environ)
  for name in ('LD_PRELOAD', 'LD_LIBRARY_PATH', 'OPENPILOT_NATIVE_REQUEST_OWNER', 'OPENPILOT_NATIVE_SCOPE_PARENT',
      'OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256', 'OPENPILOT_RUNNING_GENERATION', 'OPENPILOT_UPDATE_CHECKOUT'):
    env.pop(name, None)
  env['OPENPILOT_NATIVE_MACHINE_CONFIG'] = str(args.machine_config)
  env['OPENPILOT_NATIVE_LAUNCH_MACHINE_SHA256'] = pin
  if _BYTECODE is not None:
    _BYTECODE.cleanup()
  # Normal startup replaces this process. It is not an offroad build worker
  # whose output/lifetime should remain bounded by OwnedCommands.
  os.execve(prepared['helper'], argv, env)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--machine-config', type=Path, required=True)
  parser.add_argument('--machine-configuration-sha256')
  parser.add_argument('--launch', action='store_true')
  parser.add_argument('--paths', type=Path)
  parser.add_argument('--verify-package', action='store_true')
  parser.add_argument('--request-owner')
  parser.add_argument('--running-config', type=Path)
  parser.add_argument('--running-configuration-sha256')
  parser.add_argument('--previous')
  args = parser.parse_args()
  seed.native()
  if os.geteuid() == 0:
    raise ValueError('native source entry requires the original non-root user')
  source = args.source.resolve(strict=True)
  args.machine_config = args.machine_config.resolve(strict=True)
  if args.machine_config.is_relative_to(source):
    raise ValueError('device-local machine policy must be outside the source checkout')
  with seed.regular(args.machine_config) as stream:
    data = stream.read(4*1024**2+1)
  if len(data) > 4*1024**2:
    raise ValueError('native machine settings exceed 4 MiB')
  pin = hashlib.sha256(data).hexdigest()
  if (args.machine_configuration_sha256 != pin and (not args.launch or args.machine_configuration_sha256 is not None)):
    raise ValueError('native machine settings differ from the operation pin')
  machine = json.loads(data, object_pairs_hook=runtime.pairs)
  validate_machine(machine, source)
  if args.launch:
    if args.running_config is not None or args.running_configuration_sha256 is not None or args.previous is not None:
      raise ValueError('native startup cannot also stage an updater request')
    launch(args, machine, source, pin)
  else:
    if args.paths is not None or args.verify_package:
      raise ValueError('launcher paths/package verification require --launch')
    print(json.dumps(build_requested(args, machine, source, pin), indent=2))


if __name__ == '__main__':
  try:
    main()
  except (ValueError, TypeError, KeyError, OSError, subprocess.SubprocessError) as error:
    print('native source entry: '+str(error), file=sys.stderr)
    sys.exit(1)

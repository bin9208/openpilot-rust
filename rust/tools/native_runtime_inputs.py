"""Prepare production runtime inputs with current-source tools inside the worker.

The guarded Rust bootstrap invokes this with its verified native Python. Each
provider descriptor has an independent pin; an offline component is optional.
The old whole-runtime archive is never a fallback. This emits a full build
configuration, leaving compilation/sealing and activation as separate gates.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

if __name__ == '__main__':
  if not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
    raise SystemExit('runtime input orchestration requires Python -I -B -S')
  if sys.pycache_prefix is None or not Path(sys.pycache_prefix).is_absolute():
    raise SystemExit('runtime input orchestration requires a private absolute bytecode prefix')
  bytecode = Path(sys.pycache_prefix)
  bytecode.mkdir(exist_ok=True)
  if any(bytecode.iterdir()):
    raise SystemExit('runtime input orchestration bytecode prefix must be empty')
  sys.path.insert(0, str(Path(__file__).resolve().parent))

from native_model_inputs import InputError, digest, encoded, read_bytes, source_identity, unique_pairs, write
from native_runtime_layout import PROFILES, input_tree


def pinned(path, expected, limit=4*1024**2):
  data = read_bytes(Path(path), limit)
  import hashlib
  if not re.fullmatch('[0-9a-f]{64}', expected) or hashlib.sha256(data).hexdigest() != expected:
    raise InputError('runtime input configuration differs from its independent pin')
  return json.loads(data, object_pairs_hook=unique_pairs)


def absolute(value):
  path = Path(value)
  if not path.is_absolute() or '..' in path.parts or str(path) != value:
    raise InputError('runtime input path must use a contained absolute spelling')
  return path


def validate(configuration):
  if (set(configuration) != {'schema', 'providers', 'budgets', 'compiler_growth_bytes'}
      or configuration['schema'] != 1 or set(configuration['providers']) != set(PROFILES)
      or type(configuration['compiler_growth_bytes']) is not int
      or configuration['compiler_growth_bytes'] <= 0):
    raise InputError('runtime input preparation requires all provider roles and explicit compiler growth')
  pins = [configuration['budgets']]
  for row in configuration['providers'].values():
    if set(row) not in ({'path', 'sha256'}, {'path', 'sha256', 'offline_archive'}):
      raise InputError('provider needs its independently pinned descriptor')
    if 'offline_archive' in row:
      absolute(row['offline_archive'])
    pins.append({k: row[k] for k in ('path', 'sha256')})
  for row in pins:
    if set(row) != {'path', 'sha256'} or not re.fullmatch('[0-9a-f]{64}', row['sha256']):
      raise InputError('runtime input pin is incomplete')
    absolute(row['path'])


def pin(row):
  return dict(root=row['root'], receipt=row['tree_receipt'], sha256=row['tree_sha256'])


def compiler_record(root, build, rust, cargo, sdk_sha256):
  """Bind actual executable metadata, never a manually asserted feature list."""
  from native_model_generation import source_namespace
  reports = {Path(row['executable']): row for row in build['build']['artifacts']}
  messages = {Path(row['executable']): row for row in build['cargo_artifacts']}
  names = {'offline-qcom-compile', 'model-run', 'qcom-model-run'}
  if (len(reports) != 3 or len(build['build']['artifacts']) != 3 or set(reports) != set(messages)
      or {p.name for p in reports} != names or len(build['cargo_artifacts']) != 3):
    raise InputError('model compiler build did not produce the exact three verified executables')
  if build['build']['source'] != source_identity(root):
    raise InputError('model compiler build source differs from the current clean source')
  binaries = {}
  for path, artifact in reports.items():
    message = messages[path]
    if (sorted(message['features']) != ['default', 'native-skip-miri']
        or message['target']['name'] != path.name or message['target']['kind'] != ['bin']
        or message['profile']['test'] or digest(path, artifact['bytes']) != artifact['sha256']):
      raise InputError('model compiler actual Cargo features, target or output differ')
    binaries[path.name] = path
  compiler = binaries['offline-qcom-compile']
  metadata = messages[compiler]
  record = dict(schema=1, source=build['build']['source'],
      compiler_sha256=reports[compiler]['sha256'], compiler_bytes=reports[compiler]['bytes'],
      features=metadata['features'], profile=metadata['profile'],
      rust_toolchain_identity=rust['identity'], rust_toolchain_tree_sha256=rust['tree_sha256'],
      cargo_lock_sha256=digest(root/'rust/Cargo.lock'), compiler_sources_sha256=source_namespace(root, compiler=True),
      cargo_sources_tree_sha256=cargo['tree_sha256'], system_sdk_receipt_sha256=sdk_sha256)
  return record, binaries


def prepare(args):
  root = args.root.resolve(strict=True)
  before = source_identity(root)
  bootstrap = pinned(args.bootstrap_config, args.bootstrap_sha256)
  selection = bootstrap['runtime_inputs']
  configuration = pinned(selection['path'], selection['sha256'])
  validate(configuration)
  cache = absolute(bootstrap['cache'])
  output = absolute(str(args.output))
  if (output.exists() or not output.parent.is_dir() or output.parent.resolve() != output.parent
      or output.is_relative_to(root) or root.is_relative_to(output)
      or output.is_relative_to(cache) or cache.is_relative_to(output)):
    raise InputError('runtime input report needs a new directory outside source and caches')
  preparations = json.loads(read_bytes(args.preparations, 4*1024**2), object_pairs_hook=unique_pairs)
  if (preparations['configuration_sha256'] != args.bootstrap_sha256
      or preparations['prepared']['source']['source'] != before):
    raise InputError('runtime inputs must use the original bootstrap source and configuration')
  tools = args.tools.resolve(strict=True)
  # The Rust parent has verified the complete bundle against immutable objects.
  # Bind its executables again around every operation, including failures.
  tool_pins = {p.name: digest(p) for p in tools.iterdir() if p.name != 'manifest.json'}
  output.mkdir(mode=0o700)
  files, operations = {}, {}

  def save(name, value):
    path = output/name
    write(path, encoded(value))
    files[str(path)] = digest(path)
    return path

  def check(selected_tools=()):
    pinned(args.bootstrap_config, args.bootstrap_sha256)
    pinned(selection['path'], selection['sha256'])
    if source_identity(root) != before or any(digest(tools/name) != tool_pins[name] for name in selected_tools):
      raise InputError('runtime input source or current-source tools changed')

  def execute(name, argv):
    selected = (Path(argv[0]).name,)
    check(selected)
    path = output/(name+'.json')
    with path.open('xb') as stream:
      subprocess.run(list(map(str, argv)), stdout=stream, check=True)
      stream.flush()
      os.fsync(stream.fileno())
    check(selected)
    os.chmod(path, 0o444)
    value = json.loads(read_bytes(path, 16*1024**2), object_pairs_hook=unique_pairs)
    files[str(path)] = digest(path)
    operations[name] = value
    return value

  def asset_store(name):
    return ['--cache', cache/(name+'-assets'), '--store', cache/(name+'-prefixes')]

  preparer = tools/'openpilot-runtime-inputs'
  rust, cargo = (preparations['preparations'][k] for k in ('rust', 'cargo'))
  compiler_pins = {k: pin(v) for k, v in (('rust', rust), ('cargo', cargo))}
  compiler_pins['system-sdk'] = dict(root='/', receipt=bootstrap['system_sdk_receipt'], sha256=bootstrap['system_sdk_sha256'])
  for kind in ('rust', 'cargo'):
    input_tree(compiler_pins[kind])
  capnp = execute('capnp', [preparer, '--prepare-capnp', '--plan', root/'rust/native-capnp-assets.json', *asset_store('capnp')])
  compiler_pins['capnp'] = pin(capnp)
  codecs = execute('codecs', [tools/'stage-encoder-target', '--prepare-wheels', '--root', root,
      '--packages', 'libyuv,eigen', *asset_store('codecs')])
  compiler_pins.update({k: pin(codecs[k]) for k in ('libyuv', 'eigen')})
  raylib = execute('raylib-sdk', [tools/'stage-startup-ui-raylib', '--prepare-native-inputs',
      '--root', root, *asset_store('raylib-sdk')])
  compiler_pins['raylib'] = pin(raylib)
  roles = {}
  provider_assets, provider_store = cache/'component-assets', cache/'provider-prefixes'
  for role in PROFILES:
    row = configuration['providers'][role]
    # The preparer independently checks this same descriptor pin and the source
    # profile; source-bound roles additionally inspect their committed contract.
    argv = [tools/'build-runtime-ffmpeg', '--prepare-released-provider',
        '--plan', root/f'rust/native-{role}-provider.json',
        '--descriptor', row['path'], '--descriptor-sha256', row['sha256'],
        '--cache', provider_assets, '--store', provider_store]
    if role in ('panda', 'mpc', 'dbc', 'usbgpu'):
      argv += ['--source-root', root]
    if 'offline_archive' in row:
      argv += ['--offline-archive', row['offline_archive']]
    roles[role] = pin(execute('provider-'+role, argv))
  for role, tool in (('ffmpeg', 'build-runtime-ffmpeg'), ('opencv', 'build-runtime-opencv')):
    row = configuration['providers'][role]
    compiler_pins[role] = pin(execute(role+'-sdk', [tools/tool, '--prepare-released-prefix',
        '--header-plan', root/f'rust/native-{role}-headers.json',
        '--provider-plan', root/f'rust/native-{role}-provider.json',
        '--descriptor', row['path'], '--descriptor-sha256', row['sha256'],
        '--header-cache', cache/(role+'-headers-assets'), '--provider-cache', provider_assets,
        '--store', cache/(role+'-sdk-prefixes')]))

  inputs = {bootstrap['system_sdk_receipt']: bootstrap['system_sdk_sha256']}
  trees = {'/': bootstrap['system_sdk_receipt']}
  for row in (rust, cargo):
    inputs[row['tree_receipt']] = row['tree_sha256']
    trees[row['root']] = row['tree_receipt']
  recipe = dict(schema=1, target='aarch64-unknown-linux-gnu', packages=['openpilot-model-runtime'],
      binaries=['offline-qcom-compile', 'model-run', 'qcom-model-run'],
      features=['openpilot-model-runtime/native-skip-miri'], inputs=inputs, input_trees=trees,
      environment=dict(OPENPILOT_CARGO_VENDOR=cargo['vendor'], OPENPILOT_NATIVE_INCREMENTAL='1', TMPDIR=str(cache/'tmp')),
      expected_growth_bytes=configuration['compiler_growth_bytes'])
  recipe_path = save('model-compiler-recipe.json', recipe)
  built = execute('model-compiler-build', [tools/'openpilot-runtime-build', '--cargo-metadata',
      '--root', root, '--cache', cache/'model-compiler-build', '--toolchain', Path(rust['root'])/'bin', '--recipe', recipe_path])
  record, binaries = compiler_record(root, built, rust, cargo, bootstrap['system_sdk_sha256'])
  record_path = save('model-compiler-record.json', record)
  python = execute('python', [preparer, '--prepare-python', '--plan', root/'rust/native-python-toolchain.json', *asset_store('python')])
  if Path(sys.executable) != Path(python['python']) or sys.prefix != python['root']:
    raise InputError('runtime input orchestration is not using its independently verified Python')
  model_report = output/'models.json'
  initial_python = json.loads(read_bytes(args.preparations.parent/'python-preparation.json', 1024**2),
      object_pairs_hook=unique_pairs)
  if (initial_python['root'] != python['root'] or initial_python['tree_sha256'] != python['tree_sha256']):
    raise InputError('runtime input interpreter differs from the worker preparation')
  operations['python-initial-preparation'] = initial_python
  # Each producer gets its own initially empty bytecode namespace; none reads a
  # wheel/project pyc left by an earlier command or by the running generation.
  model_args = [python['python'], '-I', '-B', '-S', '-Xpycache_prefix='+str(output/'model-bytecode'),
      root/'rust/tools/native_model_generation.py', '--root', root, '--preparer', preparer,
      '--compiler', binaries['offline-qcom-compile'], '--compiler-build', record_path,
      '--consumer', binaries['model-run'], '--bundle-consumer', binaries['qcom-model-run'],
      '--python-cache', cache/'python-assets', '--python-store', cache/'python-prefixes',
      '--wheel-cache', cache/'model-wheel-assets', '--wheel-store', cache/'model-wheel-prefixes',
      '--environment-store', cache/'model-environments', '--model-cache', cache/'model-assets',
      '--output', cache/'model-catalogs', '--export-store', cache/'model-exports', '--report', model_report]
  check()
  subprocess.run(list(map(str, model_args)), check=True)
  check()
  models = json.loads(read_bytes(model_report, 16*1024**2), object_pairs_hook=unique_pairs)
  if models['source'] != before or not models['full_model_generation']:
    raise InputError('model preparation did not verify the complete current-source model set')
  files[str(model_report)] = digest(model_report)
  operations['models'] = models
  roles['models'] = pin(models['export'])
  roles['source-lfs'] = pin(execute('source-lfs', [preparer, '--prepare-source-lfs', '--root', root,
      '--plan', root/'rust/native-source-lfs.json', '--objects', bootstrap['stores']['objects'], *asset_store('source-lfs')]))
  raw = execute('usbgpu-model', [preparer, '--prepare-assets', '--plan', root/'rust/native-usbgpu-model.json',
      '--cache', cache/'usbgpu-model-assets'])['usbgpu-model']
  roles['usbgpu-model'] = {k: raw[k] for k in ('path', 'bytes', 'sha256')}
  budgets = pinned(configuration['budgets']['path'], configuration['budgets']['sha256'])
  budgets_path = save('budgets.json', budgets)
  spec = dict(schema=1, roles=roles, toolchain_root=rust['root'],
      libraries=['/usr/lib/aarch64-linux-gnu', '/lib/aarch64-linux-gnu'],
      expected_source_growth_bytes=budgets['expected_source_growth_bytes'],
      remaining_object_growth_bytes=budgets['remaining_object_growth_bytes'])
  spec_path = save('runtime-layout-spec.json', spec)
  composition_path = output/'composition.json'
  check()
  subprocess.run(list(map(str, [python['python'], '-I', '-B', '-S',
      '-Xpycache_prefix='+str(output/'layout-bytecode'), root/'rust/tools/native_runtime_layout.py',
      '--root', root, '--spec', spec_path, '--store', cache/'runtime-input-prefixes',
      '--verifier', preparer, '--report', composition_path])), check=True)
  check()
  composition = json.loads(read_bytes(composition_path, 16*1024**2), object_pairs_hook=unique_pairs)
  if composition['source'] != before or not composition['source_data_verified'] or composition['required_roles'] != 18:
    raise InputError('runtime composition did not verify every current-source role')
  files[str(composition_path)] = digest(composition_path)
  pins_path = save('native-input-pins.json', dict(schema=1, inputs=compiler_pins, runtime_inputs=composition['runtime_inputs']))
  catalog = execute('catalog', [tools/'openpilot-runtime-plan', '--root', root, '--cargo', Path(rust['root'])/'bin/cargo',
      '--native-inputs', pins_path, '--budgets', budgets_path, '--output', output/'catalog'])
  if catalog['source'] != before or catalog['compiler_inputs_bound'] is not True or catalog['compiled'] or catalog['activated']:
    raise InputError('runtime catalog did not bind the complete current-source compiler input set')
  for name in ('bindings.json', 'build-plan.json', 'catalog-audit.json'):
    path = output/'catalog'/name
    files[str(path)] = digest(path)
  layout_path = save('layout.json', composition['layout'])
  native_config = dict(schema=1, stores=bootstrap['stores'], cache=str(cache/'cargo-build'),
      toolchain=str(Path(rust['root'])/'bin'), build_plan=str(output/'catalog/build-plan.json'),
      layout=str(layout_path), resources=bootstrap['resources'])
  native_path = save('native-config.json', native_config)
  check(tool_pins)
  save('operations.json', operations)
  result = dict(schema=1, source=before, bootstrap_configuration_sha256=args.bootstrap_sha256,
      configuration_sha256=selection['sha256'], native_configuration_sha256=digest(native_path),
      native_configuration=str(native_path), files=files.copy(), runtime_inputs_verified=True,
      provider_roles=len(PROFILES), runtime_roles=len(roles), compiler_input_roles=len(compiler_pins),
      runtime_activated=False, runtime_started=False, full_runtime_build=False, runtime_release_downloaded=False)
  save('result.json', result)


def main():
  parser = argparse.ArgumentParser()
  for name in ('root', 'tools', 'bootstrap-config', 'preparations', 'output'):
    parser.add_argument('--'+name, type=Path, required=True)
  parser.add_argument('--bootstrap-sha256', required=True)
  args = parser.parse_args()
  if os.geteuid() == 0 or os.getgroups() or platform.machine() != 'aarch64' or Path('/VERSION').read_text().strip() != '19.8-carrot-bt1':
    raise InputError('runtime input worker requires unprivileged pinned AGNOS ARM userspace')
  prepare(args)


if __name__ == '__main__':
  main()

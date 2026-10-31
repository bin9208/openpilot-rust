"""Owned Git/descriptor fixtures exercise resolution, never a native runtime."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib
from types import SimpleNamespace
import unittest
from unittest import mock

_SPEC = importlib.util.spec_from_file_location('owned_source_entry', Path(__file__).resolve().parents[2]/'native/source_entry.py')
entry = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(entry)
runtime, seed = entry.runtime, entry.seed
_FULL_SPEC = importlib.util.spec_from_file_location('owned_full_source_entry',
    Path(__file__).resolve().parents[1]/'verify_native_source_entry.py')
full_entry = importlib.util.module_from_spec(_FULL_SPEC)
_FULL_SPEC.loader.exec_module(full_entry)
transport = full_entry.module(Path(__file__).resolve().parents[1]/'probe_native_source_transport.py', 'owned_factory_transport')
runner_policy = full_entry.module(Path(__file__).resolve().parents[1]/'runner_policy.py', 'owned_arm_runner_policy')


class Fixture:
  def __init__(self, root):
    self.root, self.source = root, root/'source'
    self.source.mkdir()
    (self.source/'rust').mkdir()
    self.offline = root/'offline'
    self.offline.mkdir()
    self.git('init', '-q')
    self.machine = dict(schema=1, stores={k: str(root/k) for k in ('sources', 'objects', 'generations', 'params')},
        cache=str(root/'cache'), seed_cache=str(root/'seed-cache'), updater_staging=str(root/'updater-staging'),
        expected_source_growth_bytes=1, expected_compile_growth_bytes=1, remaining_object_growth_bytes=1,
        compiler_growth_bytes=1, resources=dict(schema=1, memory_cgroup_parent='/sys/fs/cgroup/owned-fixture',
          maximum_memory_bytes=1, minimum_available_memory_bytes=1,
          filesystems={str(root): dict(maximum_growth_bytes=64*1024**2, minimum_free_bytes=65536)}),
        budgets=dict(schema=1, group_growth_bytes={'00': 1}, expected_source_growth_bytes=1,
          remaining_object_growth_bytes=1, estimate_basis='Owned metadata-only fixture; no build budget claim'))
    for name in ('generations', 'params'):
      (root/name).mkdir()
    (root/'params/d').mkdir()
    (root/'params/d/IsOnroad').write_bytes(b'0')
    self.plan = dict(schema=1, agnos_version=seed.AGNOS, factory_image_sha256='a'*64, roots={'usr/bin/cc': False})
    self.put('rust/native-system-sdk.json', self.plan)
    self.put('rust/native-runtime-catalog.json', dict(groups={'00': {}}))
    self.index = dict(schema=1, seed=self.asset(dict(owned_seed_descriptor=True), 'seed'),
        system_sdk=self.asset(dict(kind='agnos-system-sdk', schema=1, plan=self.plan, entries={}), 'sdk'), providers={})
    for role in entry.ROLES:
      self.put(f'rust/native-{role}-provider.json', dict(profile=role+'-fixture', source_commit='b'*40))
      self.index['providers'][role] = self.asset(dict(profile=role+'-fixture', provider_source_commit='b'*40), role)
    self.put(entry.INDEX, self.index)
    self.commit()

  def asset(self, value, role):
    data = seed.json_bytes(value)
    checksum = hashlib.sha256(data).hexdigest()
    (self.offline/(checksum+'.json')).write_bytes(data)
    return dict(url=f'https://github.com/bin9208/openpilot-rust/releases/download/owned-{role}/descriptor.json',
                bytes=len(data), sha256=checksum)

  def put(self, name, value):
    (self.source/name).write_bytes(seed.json_bytes(value))

  def git(self, *args):
    return subprocess.check_output(['/usr/bin/git', '-c', 'user.name=Owned index fixture', '-c',
        'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null',
        '-C', self.source, *args], stderr=subprocess.STDOUT)

  def commit(self):
    self.git('add', '.')
    self.git('commit', '-qm', 'owned input index fixture')

  def resolve(self, name):
    before = runtime.source_identity(self.source)
    def observe():
      if runtime.source_identity(self.source) != before:
        raise ValueError('owned source changed')
    output = self.root/'generations'/name
    output.mkdir()
    command = mock.Mock()
    command.run.side_effect = AssertionError('offline metadata fixture attempted network/process execution')
    result = entry.resolve(self.machine, self.source, output, command, observe, self.offline)
    command.run.assert_not_called()
    return result


class SourceEntryTests(unittest.TestCase):
  def test_machine_configuration_cannot_redirect_or_change_during_the_request(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      path = root/'machine.json'
      path.write_bytes(b'{"owned":1}')
      first, pin = entry.machine_configuration(path)
      self.assertEqual(first, dict(owned=1))
      path.write_bytes(b'{"owned":2}')
      with self.assertRaisesRegex(ValueError, 'operation pin'):
        entry.machine_configuration(path, pin)
      alias = root/'alias.json'
      alias.symlink_to(path)
      with self.assertRaisesRegex(ValueError, 'redirected'):
        entry.machine_configuration(alias)
      parent_alias = root/'parent-alias'
      parent_alias.symlink_to(root, target_is_directory=True)
      with self.assertRaisesRegex(ValueError, 'redirected'):
        entry.machine_configuration(parent_alias/path.name)

  def test_startup_shell_dispatches_to_the_isolated_source_entry_before_legacy_runtime_bootstrap(self):
    with tempfile.TemporaryDirectory() as temp:
      env = dict(os.environ, OPENPILOT_NATIVE_MACHINE_CONFIG=str(Path(temp)/'not-read-during-help.json'))
      root = Path(entry.__file__).resolve().parents[2]
      result = subprocess.run(['/usr/bin/bash', str(root/'launch_openpilot.sh'), '--help'], env=env,
          cwd=temp, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30, check=False)
      self.assertEqual(result.returncode, 0, result.stderr.decode())
      self.assertIn(b'--machine-config', result.stdout)
      self.assertIn(b'--launch', result.stdout)

  def startup(self, f):
    parent = f.root/'generations/configurations'
    parent.mkdir()
    data = seed.json_bytes(dict(stores=f.machine['stores']))
    pin = hashlib.sha256(data).hexdigest()
    path = parent/(pin+'.json')
    path.write_bytes(data)
    path.chmod(0o444)
    result = dict(launch_configuration=str(path), launch_configuration_sha256=pin, generation_id='a'*64)
    return entry.retain_startup(f.machine, result), result

  def test_startup_record_is_retained_atomically_and_cannot_reset_a_corrupt_record(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      self.assertIsNone(entry.startup_record(f.machine))
      record, result = self.startup(f)
      self.assertEqual(entry.startup_record(f.machine), record)
      # Another complete request cannot replace the first recorded request.
      changed = dict(result, generation_id='b'*64)
      self.assertEqual(entry.retain_startup(f.machine, changed), record)
      path = f.root/'generations/startup.json'
      path.chmod(0o644)
      path.write_bytes(b'power-loss or corrupt startup record')
      active = f.root/'generations/active.json'
      active.write_bytes(b'previous selection')
      with self.assertRaises(ValueError):
        entry.retain_startup(f.machine, result)
      self.assertEqual(path.read_bytes(), b'power-loss or corrupt startup record')
      self.assertEqual(active.read_bytes(), b'previous selection')

  def test_wrong_configuration_pin_cannot_publish_an_initial_startup_record(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      result = dict(launch_configuration=str(f.root/'unverified-config.json'),
          launch_configuration_sha256='c'*64, generation_id='a'*64)
      with self.assertRaisesRegex(ValueError, 'immutable store'):
        entry.retain_startup(f.machine, result)
      self.assertFalse((f.root/'generations/startup.json').exists())
      self.assertFalse((f.root/'generations/active.json').exists())

  def test_cached_boot_delegates_verification_without_building_or_reading_the_requested_git_index(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      record, _ = self.startup(f)
      machine_path = f.root/'machine.json'
      data = seed.json_bytes(f.machine)
      machine_path.write_bytes(data)
      pin = hashlib.sha256(data).hexdigest()
      (f.root/'params/d/IsOnroad').write_bytes(b'1')
      (f.source/'README').write_bytes(b'uncommitted update checkout')
      args = SimpleNamespace(machine_config=machine_path, request_owner=None, paths=None, verify_package=False)
      # No owned fake helper is ever executed; this test ends at the real exec
      # boundary. Generation contents are verified by the independent seed.
      with mock.patch.object(entry, 'boot_seed', return_value=dict(helper='/owned/not-executed')) as boot, \
           mock.patch.object(entry, 'build_requested', side_effect=AssertionError('existing boot rebuilt runtime')), \
           mock.patch.object(runtime, 'source_identity', side_effect=AssertionError('existing boot read mutable Git source')), \
           mock.patch.dict(os.environ, {'LD_LIBRARY_PATH': '/old-runtime', 'LD_PRELOAD': '/old-preload'}), \
           mock.patch.object(os, 'execve', side_effect=ValueError('owned exec boundary')) as execute:
        with self.assertRaisesRegex(ValueError, 'owned exec boundary'):
          entry.launch(args, f.machine, f.source, pin)
      boot.assert_called_once()
      binary, argv, env = execute.call_args.args
      self.assertEqual(binary, '/owned/not-executed')
      self.assertEqual(argv[1], '--boot-runtime')
      self.assertIn(record['initial_generation'], argv)
      self.assertEqual(env['OPENPILOT_NATIVE_LAUNCH_MACHINE_SHA256'], pin)
      self.assertNotIn('LD_LIBRARY_PATH', env)
      self.assertNotIn('LD_PRELOAD', env)
      self.assertFalse((f.root/'generations/active.json').exists())

  def test_initial_repeat_docs_and_single_provider_change_only_prepare_the_changed_descriptor(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      first = f.resolve('initial')
      self.assertTrue(all(r['offline_seed_bytes'] == r['bytes'] and r['transferred_bytes'] == 0 for r in first['receipts'].values()))
      for phase in ('repeat', 'docs'):
        if phase == 'docs':
          (f.source/'README').write_text('owned documentation change')
          f.commit()
        row = f.resolve(phase)
        self.assertEqual(row['index_sha256'], first['index_sha256'])
        self.assertTrue(all(r['stored_bytes'] == r['offline_seed_bytes'] == r['transferred_bytes'] == 0 for r in row['receipts'].values()))
      f.index['providers']['geos'] = f.asset(dict(profile='geos-updated', provider_source_commit='c'*40), 'geos-updated')
      f.put('rust/native-geos-provider.json', dict(profile='geos-updated', source_commit='c'*40))
      f.put(entry.INDEX, f.index)
      f.commit()
      changed = f.resolve('provider-changed')
      for role, row in changed['receipts'].items():
        self.assertEqual(row['stored_bytes'] > 0, role == 'geos')
        if role != 'geos':
          self.assertEqual(row['path'], first['receipts'][role]['path'])
      config = runtime.read(Path(changed['configuration']), changed['configuration_sha256'])
      self.assertEqual(config['system_sdk_sha256'], f.index['system_sdk']['sha256'])
      selection = runtime.read(Path(config['runtime_inputs']['path']), config['runtime_inputs']['sha256'])
      self.assertEqual(set(selection['providers']), set(entry.ROLES))

  def test_uncommitted_index_cannot_replace_the_committed_pin_and_partial_selection_is_rejected(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      original, checksum = entry.input_index(f.source)
      changed = copy.deepcopy(f.index)
      del changed['providers']['panda']
      f.put(entry.INDEX, changed)
      self.assertEqual(entry.input_index(f.source), (original, checksum))
      with self.assertRaises(ValueError):
        runtime.source_identity(f.source)
      f.commit()
      with self.assertRaisesRegex(ValueError, 'complete'):
        entry.input_index(f.source)

  def test_corrupt_cached_metadata_is_rejected_without_repairing_it_or_touching_previous_selection(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      first = f.resolve('initial')
      active = f.root/'generations/active.json'
      active.write_bytes(b'previous verified selection')
      path = Path(first['receipts']['seed']['path'])
      body = path.read_bytes()
      path.chmod(0o644)
      corrupt = body[:-1]+bytes([body[-1] ^ 1])
      path.write_bytes(corrupt)
      path.chmod(0o444)
      with self.assertRaisesRegex(ValueError, 'cached native metadata'):
        f.resolve('rejected')
      self.assertEqual(path.read_bytes(), corrupt)
      self.assertEqual(active.read_bytes(), b'previous verified selection')

  def test_low_space_or_budget_mismatch_cannot_start_metadata_download(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      with mock.patch.object(entry.shutil, 'disk_usage', return_value=type('Space', (), {'free': 0})()):
        with self.assertRaisesRegex(ValueError, 'insufficient budget'):
          f.resolve('low-space')
      f.machine['budgets']['group_growth_bytes'] = {'missing-current-source-group': 1}
      with self.assertRaisesRegex(ValueError, 'catalog groups'):
        f.resolve('bad-budget')

  def test_individual_metadata_files_cannot_each_reuse_the_whole_operation_growth_budget(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      f.machine['resources']['filesystems'][str(f.root)]['maximum_growth_bytes'] = f.index['seed']['bytes']+65536
      with self.assertRaisesRegex(ValueError, 'insufficient budget'):
        f.resolve('bounded-metadata')
      cache = Path(f.machine['cache'])/'metadata-assets/sha256'
      self.assertTrue((cache/f.index['seed']['sha256']/'blob').is_file())
      self.assertFalse((cache/f.index['system_sdk']['sha256']/'blob').exists())
      self.assertFalse((f.root/'generations/active.json').exists())

  def test_overlay_input_is_outside_build_stores_without_recopying_the_source(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      staged = Path(f.machine['updater_staging'])/'owned-generation/merged'
      entry.validate_machine(f.machine, staged)
      changed = copy.deepcopy(f.machine)
      changed['updater_staging'] = str(Path(changed['stores']['generations'])/'updater')
      with self.assertRaisesRegex(ValueError, 'separate real stores'):
        entry.validate_machine(changed, staged)


class PublicSourceReuseTests(unittest.TestCase):
  """Receipt rejection and real owned Git derivation; no native builds or providers."""

  def build_receipt(self):
    binaries = {f'owned-{n}': dict(sha256=f'{n:064x}', bytes=n+1, mode=0o555) for n in range(96)}
    binaries['openpilot-loggerd'] = dict(sha256='f'*64, bytes=100, mode=0o555)
    binaries['openpilot-bootlog'] = dict(sha256='e'*64, bytes=101, mode=0o555)
    binaries['openpilot-manager'] = dict(sha256='d'*64, bytes=102, mode=0o555)
    groups = {f'{n:02d}': dict(artifacts=[], cache_key='owned-cache', target_directory='/owned/target',
        recipe_sha256='a'*64, rustc_sha256='b'*64, cargo_sha256='c'*64,
        toolchain_directory='/owned/toolchain', native_tools={}, verified_input_trees={}) for n in range(13)}
    for n, name in enumerate(binaries):
      group = {'openpilot-loggerd': '07', 'openpilot-bootlog': '08', 'openpilot-manager': '09'}.get(name, f'{n % 13:02d}')
      groups[group]['artifacts'].append(dict(executable='/owned/'+name, fresh=True, **binaries[name]))
    return dict(groups=groups, binaries=binaries, stored_output_bytes=0)

  def test_same_source_and_docs_require_every_binary_fresh_and_identical(self):
    before = self.build_receipt()
    for phase in ('reused', 'launch-package', 'docs-only'):
      self.assertEqual(full_entry.verify_reuse(before, copy.deepcopy(before), phase), [])
      rebuilt = copy.deepcopy(before)
      rebuilt['groups']['04']['artifacts'][0]['fresh'] = False
      with self.assertRaisesRegex(AssertionError, 'rebuilt groups'):
        full_entry.verify_reuse(before, rebuilt, phase)
      corrupted = copy.deepcopy(before)
      corrupted['binaries']['owned-0']['sha256'] = 'c'*64
      with self.assertRaisesRegex(AssertionError, 'binary change'):
        full_entry.verify_reuse(before, corrupted, phase)
      rekeyed = copy.deepcopy(before)
      rekeyed['groups']['04']['cache_key'] = 'different-cache'
      with self.assertRaisesRegex(AssertionError, 'cache/input identity'):
        full_entry.verify_reuse(before, rekeyed, phase)
    missing = copy.deepcopy(before)
    del missing['groups']['12']
    with self.assertRaises(AssertionError):
      full_entry.verify_reuse(before, missing, 'docs-only')

  def test_logger_stamp_consumers_match_the_committed_catalog_and_direct_dependency(self):
    root = Path(entry.__file__).resolve().parents[2]
    catalog = json.loads((root/'rust/native-runtime-catalog.json').read_text())
    for group, binary in full_entry.LOGGER_CHANGE_CONSUMERS.items():
      self.assertIn(binary, catalog['groups'][group]['binaries'])
    manifests = {path.parent.resolve(): tomllib.loads(path.read_text())
                 for path in (root/'rust/crates').glob('*/Cargo.toml')}
    reverse = {}
    for directory, manifest in manifests.items():
      dependencies = [manifest.get('dependencies', {})]
      dependencies += [target.get('dependencies', {}) for target in manifest.get('target', {}).values()]
      for section in dependencies:
        for dependency in section.values():
          if isinstance(dependency, dict) and 'path' in dependency:
            origin = manifests.get((directory/dependency['path']).resolve())
            if origin:
              reverse.setdefault(origin['package']['name'], set()).add(
                  (manifest['package']['name'], dependency.get('optional', False)))
    closure = {'openpilot-loggerd'}
    while True:
      consumers = {(name, optional) for dependency in closure for name, optional in reverse.get(dependency, set())}
      self.assertTrue(all(not optional for _, optional in consumers))
      expanded = closure | {name for name, _ in consumers}
      if expanded == closure:
        break
      closure = expanded
    self.assertEqual(closure, set(full_entry.LOGGER_CHANGE_CONSUMERS.values()))
    self.assertIn('environment.init_data()', (root/'rust/crates/bootlog/src/lib.rs').read_text())
    build = (root/'rust/crates/loggerd/build.rs').read_text()
    self.assertIn('cargo:rerun-if-changed=src', build)
    self.assertIn('cargo:rustc-env=LOGGERD_SOURCE_COMMIT={commit}', build)
    self.assertIn('env!("LOGGERD_SOURCE_COMMIT")', (root/'rust/crates/loggerd/src/metadata.rs').read_text())

  def test_small_logger_requires_the_three_consumers_but_manager_elf_may_stay_identical(self):
    before = self.build_receipt()
    after = copy.deepcopy(before)
    for group, binary in full_entry.LOGGER_CHANGE_CONSUMERS.items():
      next(a for a in after['groups'][group]['artifacts']
           if Path(a['executable']).name == binary)['fresh'] = False
    with self.assertRaisesRegex(AssertionError, 'binary change'):
      full_entry.verify_reuse(before, after, 'small-logger')
    after['binaries']['openpilot-loggerd']['sha256'] = 'c'*64
    with self.assertRaisesRegex(AssertionError, 'binary change'):
      full_entry.verify_reuse(before, after, 'small-logger')
    after['binaries']['openpilot-bootlog']['sha256'] = 'd'*64
    self.assertEqual(full_entry.verify_reuse(before, after, 'small-logger'), ['07', '08', '09'])
    manager_changed = copy.deepcopy(after)
    manager_changed['binaries']['openpilot-manager']['sha256'] = 'c'*64
    self.assertEqual(full_entry.verify_reuse(before, manager_changed, 'small-logger'), ['07', '08', '09'])
    for group, binary in full_entry.LOGGER_CHANGE_CONSUMERS.items():
      missing = copy.deepcopy(after)
      for artifact in missing['groups'][group]['artifacts']:
        artifact['fresh'] = True
      with self.assertRaisesRegex(AssertionError, 'rebuilt groups'):
        full_entry.verify_reuse(before, missing, 'small-logger')
      unchanged = copy.deepcopy(after)
      unchanged['binaries'][binary] = before['binaries'][binary]
      if binary in full_entry.LOGGER_REQUIRED_CHANGES:
        with self.assertRaisesRegex(AssertionError, 'binary change'):
          full_entry.verify_reuse(before, unchanged, 'small-logger')
    for binary in ('owned-7', 'owned-8', 'owned-9', 'owned-0'):
      changed = copy.deepcopy(after)
      changed['binaries'][binary]['sha256'] = 'c'*64
      with self.assertRaisesRegex(AssertionError, 'binary change'):
        full_entry.verify_reuse(before, changed, 'small-logger')
      rebuilt = copy.deepcopy(after)
      for group in rebuilt['groups'].values():
        for artifact in group['artifacts']:
          if Path(artifact['executable']).name == binary:
            artifact['fresh'] = False
      with self.assertRaises(AssertionError):
        full_entry.verify_reuse(before, rebuilt, 'small-logger')
    outside = copy.deepcopy(after)
    outside['groups']['10']['artifacts'][0]['fresh'] = False
    with self.assertRaisesRegex(AssertionError, 'rebuilt groups'):
      full_entry.verify_reuse(before, outside, 'small-logger')
    del after['binaries']['openpilot-bootlog']
    with self.assertRaises(AssertionError):
      full_entry.verify_reuse(before, after, 'small-logger')

  def test_sealed_logger_help_prefers_its_libraries_and_rejects_loader_failure(self):
    with tempfile.TemporaryDirectory() as temp:
      state = Path(temp)
      logger = state/'sealed/rust/bin/openpilot-loggerd'
      deadline = time.monotonic_ns()+60_000_000_000
      def help_output(*args, **kwargs):
        kwargs['stdout'].write(full_entry.REUSE_LOGGER_HELP.encode())
      with mock.patch.dict(os.environ, {'LD_LIBRARY_PATH': '/unverified'}), \
           mock.patch.object(full_entry.subprocess, 'run', side_effect=help_output) as run:
        full_entry.verify_logger_help(entry, logger, state, deadline)
      self.assertEqual(run.call_args.args[0], [str(logger), '--help'])
      self.assertEqual(run.call_args.kwargs['env']['LD_LIBRARY_PATH'], str(logger.parent/'lib'))
      self.assertIs(run.call_args.kwargs['check'], True)
      failure = state/'loader-failure'
      failure.mkdir()
      with mock.patch.object(full_entry.subprocess, 'run', side_effect=subprocess.CalledProcessError(127, str(logger))):
        with self.assertRaises(subprocess.CalledProcessError):
          full_entry.verify_logger_help(entry, logger, failure, deadline)

  def test_asset_aggregate_is_not_counted_again_and_missing_or_invalid_counters_fail(self):
    row = dict(models=dict(transferred_bytes=7, model_assets={'raw': dict(transferred_bytes=7)}),
               seed=dict(transferred_bytes=dict(helper=2, notices=1)))
    self.assertEqual(sum(full_entry.transfer_counts(row).values()), 10)
    for row in ({'bytes': 1}, {'transferred_bytes': {}}, {'transferred_bytes': True},
                {'transferred_bytes': -1}, {'transferred_bytes': '0'}):
      with self.assertRaises(AssertionError):
        full_entry.transfer_counts(row)

  def test_warm_asset_zero_cannot_hide_a_metadata_download_or_different_pins(self):
    before = dict(input_index_sha256='a'*64, machine_configuration_sha256='b'*64,
                  sealed_build=self.build_receipt())
    after = dict(copy.deepcopy(before), large_asset_transferred_bytes=0,
                 metadata_transferred_bytes={'seed': 0})
    full_entry.verify_warm_reuse(before, after, 'docs-only')
    for field, value in (('large_asset_transferred_bytes', 1),
                         ('metadata_transferred_bytes', {'seed': 1}),
                         ('input_index_sha256', 'c'*64),
                         ('machine_configuration_sha256', 'd'*64)):
      changed = dict(after, **{field: value})
      with self.assertRaises(AssertionError):
        full_entry.verify_warm_reuse(before, changed, 'docs-only')

  def test_derived_commits_are_clean_separate_sources_preserving_the_original_index(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      (f.source/'README.md').write_text('Owned base documentation.\n')
      logger = f.source/'rust/crates/loggerd/src/main.rs'
      logger.parent.mkdir(parents=True)
      logger.write_text(full_entry.LOGGER_HELP+'\n')
      f.commit()
      original = runtime.source_identity(f.source)
      index, pin = entry.input_index(f.source)
      request = dict(source=original, index_sha256=pin,
                     estimates=dict(expected_source_growth_bytes=1, minimum_free_bytes=1))
      derived = f.root/'derived-source'
      deadline_ns = time.monotonic_ns()+60_000_000_000
      with self.assertRaises(AssertionError):
        full_entry.derive_source(entry, f.source, f.source, 'small-logger', request, deadline_ns)
      docs = full_entry.derive_source(entry, f.source, derived, 'docs-only', request, deadline_ns)
      self.assertEqual(docs['parent_source'], original)
      self.assertNotEqual(docs['source'], original)
      entry.validate_machine(f.machine, derived)
      small = full_entry.derive_source(entry, f.source, derived, 'small-logger', request, deadline_ns)
      self.assertEqual(small['parent_source'], docs['source'])
      self.assertNotEqual(small['source'], original)
      self.assertEqual(runtime.source_identity(derived), small['source'])
      self.assertEqual(runtime.source_identity(f.source), original)
      self.assertEqual(entry.input_index(derived), (index, pin))
      self.assertEqual(logger.read_text(), full_entry.LOGGER_HELP+'\n')
      self.assertIn(full_entry.REUSE_LOGGER_HELP, (derived/'rust/crates/loggerd/src/main.rs').read_text())
      # The original pin is retained, rather than relabeling derived results as P.
      (derived/'README.md').write_text('uncommitted change')
      with self.assertRaises(ValueError):
        full_entry.derive_source(entry, f.source, derived, 'small-logger', request, deadline_ns)

  def test_deadline_is_not_reset_for_derived_git_commands(self):
    with mock.patch.object(full_entry.subprocess, 'run') as run:
      with self.assertRaises(TimeoutError):
        full_entry.reuse_git(entry, time.monotonic_ns()-1, 'status')
      run.assert_not_called()

  def test_resource_receipt_requires_its_real_source_policy_and_successful_kernel_limits(self):
    with tempfile.TemporaryDirectory() as temp:
      parent = Path(temp)
      attempt = parent/'attempt-owned'
      attempt.mkdir()
      path = attempt/'resources.jsonl'
      source = dict(commit='a'*40, tree='b'*40)
      budget = dict(maximum_growth_bytes=10, minimum_free_bytes=20)
      policy = dict(maximum_memory_bytes=100, filesystems={str(parent): budget})
      row = dict(state='success', elapsed_ms=3, policy=policy,
          context=dict(requested_source=source, configuration_sha256='c'*64),
          memory=dict(kernel_peak_bytes=50, limit_events=0),
          filesystems=[dict(budget=budget, observed_peak_growth_bytes=5, minimum_available_bytes=25)])
      def write(value):
        path.write_text(json.dumps(dict(state='admitted'))+'\n'+json.dumps(value)+'\n')
      write(row)
      result = full_entry.resource_receipt(entry, attempt, parent, source, 'c'*64, policy)
      self.assertEqual(result['sha256'], full_entry.checksum(path))
      for changes in (dict(state='failure'), dict(context=dict(row['context'], requested_source=dict(source, tree='d'*40))),
                      dict(memory=dict(kernel_peak_bytes=50, limit_events=1)),
                      dict(memory=dict(kernel_peak_bytes=101, limit_events=0)),
                      dict(filesystems=[dict(row['filesystems'][0], minimum_available_bytes=19)]),
                      dict(filesystems=[dict(row['filesystems'][0], observed_peak_growth_bytes=11)])):
        write(dict(row, **changes))
        with self.assertRaises(AssertionError):
          full_entry.resource_receipt(entry, attempt, parent, source, 'c'*64, policy)
      write(row)
      with self.assertRaises(AssertionError):
        full_entry.resource_receipt(entry, attempt, parent, source, 'e'*64, policy)


class NativeArmAdmissionTests(unittest.TestCase):
  def test_agnos_requires_hosted_linux_arm_and_scope_preserves_registered_x64(self):
    hosted = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted', RUNNER_OS='Linux', RUNNER_ARCH='ARM64')
    runner_policy.require_github_native_arm(env=hosted)
    runner_policy.require_resource_scope_environment(env=hosted)
    self.assertFalse(runner_policy.environment_allowed(hosted, self_hosted_only=True))
    private = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='bin-actions3')
    runner_policy.require_resource_scope_environment(env=private)
    with self.assertRaises(ValueError):
      runner_policy.require_github_native_arm(env=private)
    for changed in (dict(RUNNER_ARCH='X64'), dict(RUNNER_OS='Windows'), dict(GITHUB_ACTIONS='false'),
                    dict(RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='unknown')):
      env = dict(hosted, **changed)
      with self.assertRaises(ValueError):
        runner_policy.require_github_native_arm(env=env)
      with self.assertRaises(ValueError):
        runner_policy.require_resource_scope_environment(env=env)
    for missing in ('RUNNER_OS', 'RUNNER_ARCH'):
      env = dict(hosted)
      del env[missing]
      with self.assertRaises(ValueError):
        runner_policy.require_github_native_arm(env=env)

  def test_factory_mapping_preserves_root_comma_and_other_ids_without_overlaps(self):
    for host in (1000, 1001):
      ranges = [list(map(int, row.split())) for row in transport.factory_id_map(host).splitlines()]
      guest_ids, host_ids = set(), set()
      for inner, outer, length in ranges:
        a, b = set(range(inner, inner+length)), set(range(outer, outer+length))
        self.assertFalse(guest_ids & a)
        self.assertFalse(host_ids & b)
        guest_ids.update(a)
        host_ids.update(b)
      self.assertEqual(guest_ids, set(range(65536)))
      self.assertEqual(host_ids, set(range(65536)))
      for original in (0, 5, 999, 1000, 1001, 1002, 65534):
        mapped = transport.factory_host_id(original, host)
        actual = next(inner+mapped-outer for inner, outer, length in ranges if outer <= mapped < outer+length)
        self.assertEqual(actual, original)
      self.assertEqual(transport.factory_host_id(1000, host), host)
    for host in (0, 999, 1002, 65536, True, '1001'):
      with self.assertRaises(ValueError):
        transport.factory_id_map(host)

  def test_factory_owner_remap_refuses_a_shared_or_redirected_tree_before_chown(self):
    with tempfile.TemporaryDirectory() as temporary:
      job = Path(temporary)
      original = job/'shared-source'
      original.mkdir()
      alias = job/'source-entry-full'
      alias.mkdir()
      (alias/'factory').symlink_to(original, target_is_directory=True)
      with mock.patch.dict(os.environ, CI_JOB_TEMP=str(job)), \
           mock.patch.object(transport.os, 'geteuid', return_value=0), \
           mock.patch.object(transport.os, 'chown') as chown, \
           mock.patch.object(transport.os, 'fchown') as fchown:
        for candidate in (original, alias/'factory'):
          with self.assertRaises(ValueError):
            transport.remap_factory_owners(candidate, 1001, 1001)
        chown.assert_not_called()
        fchown.assert_not_called()


if __name__ == '__main__':
  unittest.main()

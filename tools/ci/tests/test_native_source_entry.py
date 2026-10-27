"""Owned Git/descriptor fixtures exercise resolution, never a native runtime."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

_SPEC = importlib.util.spec_from_file_location('owned_source_entry', Path(__file__).resolve().parents[2]/'native/source_entry.py')
entry = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(entry)
runtime, seed = entry.runtime, entry.seed


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

  def test_overlay_input_is_outside_build_stores_without_recopying_the_source(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      staged = Path(f.machine['updater_staging'])/'owned-generation/merged'
      entry.validate_machine(f.machine, staged)
      changed = copy.deepcopy(f.machine)
      changed['updater_staging'] = str(Path(changed['stores']['generations'])/'updater')
      with self.assertRaisesRegex(ValueError, 'separate real stores'):
        entry.validate_machine(changed, staged)


if __name__ == '__main__':
  unittest.main()

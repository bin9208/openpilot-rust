"""Owned archives exercise admission/preservation, never actual NumPy or models."""
import base64
import copy
import csv
import fcntl
import hashlib
import io
import json
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_model_inputs as inputs


def wheel(name, p, seed=1, case='valid'):
  dist = f'{name}-{p["version"]}.dist-info'
  elf = bytearray(80)
  elf[:7] = b'\x7fELF\x02\x01\x01'
  elf[18:20] = (62 if case == 'wrong-elf' else 183).to_bytes(2, 'little')
  elf[20:24] = (1).to_bytes(4, 'little')
  elf[52:54] = (64).to_bytes(2, 'little')
  elf[-1] = seed
  tags = p['filename'].removesuffix('.whl').split('-', 4)[4].split('.')
  metadata = f'Name: {name if case != "wrong-name" else "other"}\nVersion: {p["version"]}\n'
  if name == 'zstandard': metadata += 'Requires-Dist: cffi>=1.17; extra == "cffi"\n'
  if case == 'dependency': metadata += 'Requires-Dist: undeclared>=1\n'
  files = {
      name+'/__init__.py': f'# owned fixture {seed}, never imported\n'.encode(),
      name+'/owned.cpython-312-aarch64-linux-gnu.so': bytes(elf),
      dist+'/METADATA': metadata.encode(),
      dist+'/WHEEL': ('Wheel-Version: 1.0\nRoot-Is-Purelib: false\n'+''.join(
          'Tag: cp312-cp312-'+('win_amd64' if case == 'wrong-tag' else tag)+'\n' for tag in tags)).encode(),
      dist+'/licenses/LICENSE': b'owned license fixture',
  }
  if case == 'missing': files.pop(dist+'/WHEEL')
  if case == 'no-notice': files.pop(dist+'/licenses/LICENSE')
  if case == 'traversal': files[name+'/../../escape'] = b'not allowed'
  if case == 'unrelated': files['shadow_module.py'] = b'not allowed'
  record = io.StringIO()
  writer = csv.writer(record, lineterminator='\n')
  for path, value in files.items():
    digest = base64.urlsafe_b64encode(hashlib.sha256(value).digest()).rstrip(b'=').decode()
    writer.writerow([path, 'sha256='+('x'*43 if case == 'record' else digest), len(value)])
  writer.writerow([dist+'/RECORD', '', ''])
  files[dist+'/RECORD'] = record.getvalue().encode()
  result = io.BytesIO()
  with zipfile.ZipFile(result, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
    for path, value in files.items():
      entry = zipfile.ZipInfo(path)
      entry.compress_type = zipfile.ZIP_DEFLATED
      entry.external_attr = (stat.S_IFREG | 0o644) << 16
      if case == 'symlink' and path.endswith('__init__.py'):
        entry.external_attr = (stat.S_IFLNK | 0o777) << 16
      if case == 'special' and path.endswith('__init__.py'):
        entry.external_attr = (stat.S_IFREG | 0o4644) << 16
      archive.writestr(entry, value)
  return result.getvalue()


class Fixture:
  def __init__(self, base):
    self.base = base
    self.plan = json.loads((Path(__file__).resolve().parents[2]/'native-model-wheels.json').read_text())
    self.receipts = {}
    self.store, self.environment = base/'packages', base/'environments'
    for name in self.plan['packages']: self.replace(name)

  def replace(self, name, seed=1, case='valid'):
    p = self.plan['packages'][name]
    data = wheel(name, p, seed, case)
    p['asset']['sha256'], p['asset']['bytes'] = inputs.sha(data), len(data)
    path = self.base/(p['asset']['sha256']+'.whl')
    path.write_bytes(data)
    self.receipts['model-wheel-'+name] = dict(sha256=p['asset']['sha256'], bytes=len(data), path=str(path))

  def prepare(self):
    return inputs.prepare(self.plan, self.receipts, self.store, self.environment)

  def verify(self, result):
    inputs.verify(inputs.read_json(Path(result['tree_receipt']), 16*1024**2), Path(result['root']))


class ModelWheelTests(unittest.TestCase):
  def test_repeated_and_estimate_only_changes_need_no_payload_copy_or_new_links(self):
    with tempfile.TemporaryDirectory() as temporary:
      f = Fixture(Path(temporary))
      first = f.prepare()
      self.assertGreater(first['copied_bytes'], 0)
      self.assertGreater(first['linked_bytes'], 0)
      f.plan['packages']['numpy']['maximum_unpacked_bytes'] += 1024**2
      repeated = f.prepare()
      self.assertEqual((first['root'], first['tree_sha256']), (repeated['root'], repeated['tree_sha256']))
      self.assertEqual(repeated['copied_bytes']+repeated['linked_bytes'], 0)
      self.assertFalse(repeated['installer_executed'] or repeated['wheels_imported'] or repeated['runtime_activated'])
      self.assertEqual((Path(first['site_packages'])/'numpy/__init__.py').stat().st_ino,
                       (Path(first['packages']['numpy']['root'])/'site-packages/numpy/__init__.py').stat().st_ino)
      f.verify(first)

  def test_one_wheel_change_preserves_unrelated_prefix_inode_and_the_previous_environment(self):
    with tempfile.TemporaryDirectory() as temporary:
      f = Fixture(Path(temporary))
      first = f.prepare()
      old = (Path(first['site_packages'])/'numpy/__init__.py').read_bytes()
      f.replace('numpy', seed=2)
      changed = f.prepare()
      self.assertNotEqual(first['root'], changed['root'])
      self.assertGreater(changed['packages']['numpy']['copied_bytes'], 0)
      self.assertEqual(changed['packages']['zstandard']['copied_bytes'], 0)
      self.assertEqual(first['packages']['zstandard']['root'], changed['packages']['zstandard']['root'])
      self.assertEqual((Path(first['site_packages'])/'zstandard/__init__.py').stat().st_ino,
                       (Path(changed['site_packages'])/'zstandard/__init__.py').stat().st_ino)
      self.assertEqual((Path(first['site_packages'])/'numpy/__init__.py').read_bytes(), old)
      f.verify(first)
      f.verify(changed)

  def test_authenticated_but_wrong_wheel_metadata_code_record_paths_and_modes_leave_old_environment(self):
    for case in ('wrong-name', 'dependency', 'wrong-tag', 'missing', 'no-notice', 'traversal',
                 'unrelated', 'record', 'wrong-elf', 'symlink', 'special'):
      with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
        f = Fixture(Path(temporary))
        first = f.prepare()
        f.replace('numpy', seed=2, case=case)
        with self.assertRaises(inputs.InputError): f.prepare()
        f.verify(first)
        self.assertFalse((f.base/'escape').exists())
        self.assertEqual(len(list(f.environment.glob('*/generation'))), 1)

  def test_forged_receipt_extra_file_changed_mode_and_bad_compressed_cache_are_not_repaired(self):
    for case in ('receipt', 'extra', 'mode', 'archive'):
      with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
        f = Fixture(Path(temporary))
        first = f.prepare()
        if case == 'receipt':
          path = Path(first['tree_receipt'])
          path.chmod(0o644)
          path.write_text('{}')
        elif case == 'extra':
          (Path(first['root'])/'untrusted').write_bytes(b'extra')
        elif case == 'mode':
          (Path(first['site_packages'])/'numpy/__init__.py').chmod(0o755)
        else:
          Path(f.receipts['model-wheel-numpy']['path']).write_bytes(b'corrupted')
        with self.assertRaises(inputs.InputError): f.prepare()
        if case == 'receipt': self.assertEqual(path.read_text(), '{}')
        if case == 'extra': self.assertEqual((Path(first['root'])/'untrusted').read_bytes(), b'extra')
        if case == 'mode': self.assertEqual(stat.S_IMODE((Path(first['site_packages'])/'numpy/__init__.py').stat().st_mode), 0o755)
        if case == 'archive': f.verify(first)

  def test_lock_conflict_preserves_complete_inputs_and_releases_for_a_retry(self):
    with tempfile.TemporaryDirectory() as temporary:
      f = Fixture(Path(temporary))
      first = f.prepare()
      lock_path = Path(first['packages']['numpy']['root']).parent.parent/'lock'
      with lock_path.open('wb') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        with self.assertRaises(BlockingIOError): f.prepare()
        f.verify(first)
      self.assertEqual(f.prepare()['copied_bytes'], 0)

  def test_capacity_failure_and_abandoned_pending_directory_preserve_the_previous_generation(self):
    with tempfile.TemporaryDirectory() as temporary:
      f = Fixture(Path(temporary))
      first = f.prepare()
      f.replace('numpy', seed=2)
      with mock.patch.object(inputs.shutil, 'disk_usage', return_value=SimpleNamespace(free=0)):
        with self.assertRaises(inputs.InputError): f.prepare()
      f.verify(first)
      identity = inputs.sha(inputs.encoded(inputs.package_identity('numpy', f.plan['packages']['numpy'])))
      pending = f.store/identity/'.pending-abandoned'
      pending.mkdir()
      (pending/'marker').write_bytes(b'owned interrupted attempt')
      changed = f.prepare()
      f.verify(first)
      f.verify(changed)
      self.assertEqual((pending/'marker').read_bytes(), b'owned interrupted attempt')

  def test_plan_bounds_missing_package_unknown_locator_and_in_source_paths_are_rejected(self):
    with tempfile.TemporaryDirectory() as temporary:
      f = Fixture(Path(temporary))
      for case in ('missing', 'target', 'locator', 'size'):
        plan = copy.deepcopy(f.plan)
        if case == 'missing': plan['packages'].pop('zstandard')
        elif case == 'target': plan['target'] = 'x86_64-unknown-linux-gnu'
        elif case == 'locator': plan['packages']['numpy']['asset']['url'] = 'https://example.invalid/object.whl'
        else: plan['packages']['numpy']['maximum_unpacked_bytes'] = 0
        with self.assertRaises(inputs.InputError): inputs.prepare(plan, f.receipts, f.store, f.environment)
      self.assertFalse(f.store.exists() or f.environment.exists())
      source = f.base/'source'
      source.mkdir()
      with self.assertRaises(inputs.InputError): inputs.outside(source, source/'cache')
      redirected = f.base/'redirected'
      redirected.symlink_to(source, target_is_directory=True)
      with self.assertRaises(inputs.InputError): inputs.outside(source, redirected)


if __name__ == '__main__':
  unittest.main()

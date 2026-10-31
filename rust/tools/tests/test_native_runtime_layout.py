"""Owned tree/alias fixtures; full real composition runs only in genuine CI."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_runtime_layout import ROLES, compose, input_tree
from native_model_inputs import InputError, encoded, sha


class RuntimeLayoutTests(unittest.TestCase):
  def fixture(self, base):
    root = base/'prefix'
    (root/'lib').mkdir(parents=True)
    data = b'owned foreign data, not an actual ELF'
    (root/'lib/owned.so').write_bytes(data)
    (root/'lib/owned.so').chmod(0o444)
    (root/'lib/owned.so.1').symlink_to('owned.so')
    tree = dict(schema=1, entries={'lib': dict(kind='directory'),
        'lib/owned.so': dict(kind='file', mode=0o444, bytes=len(data), sha256=sha(data)),
        'lib/owned.so.1': dict(kind='symlink', target='owned.so')})
    receipt = base/'tree.json'
    receipt.write_bytes(encoded(tree))
    return dict(root=str(root), receipt=str(receipt), sha256=sha(encoded(tree))), tree

  def test_complete_roster_is_required_before_any_store_or_source_operation(self):
    with tempfile.TemporaryDirectory() as temporary:
      base = Path(temporary)
      for roles in ({}, {'unknown': {}}, {role: {} for role in ROLES if role != 'usbgpu-model'}):
        spec = dict(schema=1, roles=roles, toolchain_root='/unused', libraries=[],
                    expected_source_growth_bytes=1, remaining_object_growth_bytes=1)
        with self.assertRaises(InputError):
          compose(base/'absent-source', spec, base/'must-not-create', base/'absent-verifier')
        self.assertFalse((base/'must-not-create').exists())

  def test_verified_internal_alias_is_admitted_but_link_escape_is_not(self):
    with tempfile.TemporaryDirectory() as temporary:
      base = Path(temporary)
      pin, tree = self.fixture(base)
      input_tree(pin)
      (base/'outside').write_bytes(b'owned foreign data, not an actual ELF')
      link = Path(pin['root'])/'lib/owned.so.1'
      link.unlink()
      link.symlink_to('../../outside')
      tree['entries']['lib/owned.so.1']['target'] = '../../outside'
      Path(pin['receipt']).write_bytes(encoded(tree))
      pin['sha256'] = sha(encoded(tree))
      with self.assertRaises(InputError):
        input_tree(pin)

  def test_data_mode_receipt_and_additions_are_checked_without_healing_inputs(self):
    with tempfile.TemporaryDirectory() as temporary:
      base = Path(temporary)
      pin, _ = self.fixture(base)
      data = Path(pin['root'])/'lib/owned.so'
      data.chmod(0o644)
      with self.assertRaises(InputError):
        input_tree(pin)
      data.chmod(0o444)
      added = Path(pin['root'])/'unexpected'
      added.mkdir()
      with self.assertRaises(InputError):
        input_tree(pin)
      self.assertTrue(added.is_dir())
      added.rmdir()
      bad = {**pin, 'sha256': '0'*64}
      with self.assertRaises(InputError):
        input_tree(bad)
      input_tree(pin)


if __name__ == '__main__':
  unittest.main()

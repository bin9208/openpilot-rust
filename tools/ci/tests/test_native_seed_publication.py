"""Synthetic export envelopes bind existing ELF bytes; never execute a helper."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location('owned_seed_publication', ROOT/'tools/native/publish_seed.py')
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)


class SeedPublicationTests(unittest.TestCase):
  def fixture(self, root, fault=None):
    index = json.loads((ROOT/'rust/native-bootstrap-inputs.json').read_text())
    descriptor = index['seed']['sha256']
    raw = (ROOT/'native-inputs/metadata'/(descriptor+'.json')).read_bytes()
    plan = json.loads(raw)
    source = plan['source']['commit']
    proof = dict(kind='release-seed-export', source=copy.deepcopy(plan['source']), workflow_source=copy.deepcopy(plan['source']),
        cleanup_verified=True, native_arm_execution=True, emulated=False, source_entry_full_build_executed=False,
        runtime_activated=False, manager_started=False, exported=dict(descriptor_sha256=descriptor),
        release_artifact=dict(sha256=plan['assets']['helper']['sha256'], bytes=plan['assets']['helper']['bytes'],
          executable='/owned/release/openpilot-runtime-inputs'))
    if fault == 'source':
      proof['source']['commit'] = 'f'*40
    if fault == 'cleanup':
      proof['cleanup_verified'] = False
    if fault == 'debug':
      proof['release_artifact']['executable'] = '/owned/debug/openpilot-runtime-inputs'
    path = root/'export.zip'
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
      archive.writestr('source-entry-full/result.json', json.dumps(proof))
      archive.writestr(publisher.PREFIX+'manifest.json', raw)
      archive.writestr(publisher.PREFIX+'manifest.sha256', descriptor+'\n')
      for name in publisher.seed.FILES.values():
        data = (ROOT/'native-inputs/seeds'/descriptor/name).read_bytes()
        if fault == 'bytes' and name == publisher.seed.FILES['helper']:
          data = data[:-1]+bytes([data[-1]^1])
        archive.writestr(publisher.PREFIX+name, data)
    return publisher.publication.Bundle(path), source, descriptor

  def test_export_bytes_and_source_are_bound_before_any_release_mutation(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      bundle, source, descriptor = self.fixture(root)
      try:
        plan, actual, rows = publisher.validate(bundle, source, root/'verified')
      finally:
        bundle.close()
      self.assertEqual(actual, descriptor)
      self.assertEqual(plan['source']['commit'], source)
      self.assertEqual(len(rows), 4)
      self.assertTrue(all(hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256'] for row in rows))

  def test_changed_source_cleanup_profile_or_helper_bytes_are_rejected(self):
    for fault in ('source', 'cleanup', 'debug', 'bytes'):
      with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        bundle, source, _ = self.fixture(root, fault)
        try:
          with self.assertRaises(ValueError):
            publisher.validate(bundle, source, root/'verified')
        finally:
          bundle.close()


if __name__ == '__main__':
  unittest.main()

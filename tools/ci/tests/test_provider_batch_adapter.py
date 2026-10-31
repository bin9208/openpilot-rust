"""Batch selectors retain the actual export paths bound into AGNOS guests."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('provider_batch_adapter', Path(__file__).parents[1]/'native_provider_batch.py')
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class AdapterTests(unittest.TestCase):
  def fixture(self, work):
    archive, descriptor = work/'component.tar.gz', work/'provider.json'
    archive.write_bytes(b'owned validated component fixture')
    descriptor.write_bytes(b'{"schema":1}')
    value = dict(descriptor=str(descriptor), archive=str(archive),
        descriptor_sha256=hashlib.sha256(descriptor.read_bytes()).hexdigest(),
        component_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(), component_bytes=archive.stat().st_size,
        provider_source_commit='a'*40, provider_executed=False, published_release=False)
    receipt = work/'batch.json'
    receipt.write_text(json.dumps(dict(schema=1, source_archive_sha256='b'*64, exports={'owned':value})))
    plan = work/'plan.json'
    plan.write_text(json.dumps(dict(profile='owned', source_commit='a'*40, source_archive=dict(sha256='b'*64))))
    env = dict(NATIVE_PROVIDER_BATCH=str(receipt), NATIVE_PROVIDER_BATCH_SHA256=hashlib.sha256(receipt.read_bytes()).hexdigest())
    return plan, env, value

  def test_guest_view_has_identical_pinned_files_at_the_legacy_bind_path(self):
    with tempfile.TemporaryDirectory() as temporary:
      work = Path(temporary)
      plan, env, original = self.fixture(work)
      with mock.patch.dict(os.environ, env):
        value = adapter.export_component('not-executed', plan, work, work, work/'result.json',
            work/'legacy-store', work/'guest-export')
      self.assertEqual(value['descriptor'], str(work/'guest-export/provider.json'))
      self.assertEqual(value['archive'], str(work/'guest-export/component.tar.gz'))
      for field in ('descriptor', 'archive'):
        self.assertEqual(Path(value[field]).read_bytes(), Path(original[field]).read_bytes())
        self.assertEqual(Path(value[field]).stat().st_mode & 0o222, 0)
      self.assertEqual(json.loads((work/'result.json').read_text()), value)

  def test_existing_view_and_receipt_are_preserved_and_corrupt_inputs_rejected(self):
    for fault in ('view-exists', 'receipt-exists', 'archive-damaged'):
      with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        plan, env, original = self.fixture(work)
        if fault == 'view-exists':
          (work/'guest-export').mkdir()
          (work/'guest-export/sentinel').write_bytes(b'previous good output')
        elif fault == 'receipt-exists': (work/'result.json').write_bytes(b'previous good receipt')
        else: Path(original['archive']).write_bytes(b'tampered')
        with mock.patch.dict(os.environ, env), self.assertRaises((ValueError, FileExistsError)):
          adapter.export_component('not-executed', plan, work, work, work/'result.json',
              work/'legacy-store', work/'guest-export')
        if fault == 'view-exists': self.assertEqual((work/'guest-export/sentinel').read_bytes(), b'previous good output')
        else: self.assertFalse((work/'guest-export').exists())
        if fault == 'receipt-exists': self.assertEqual((work/'result.json').read_bytes(), b'previous good receipt')


if __name__ == '__main__': unittest.main()

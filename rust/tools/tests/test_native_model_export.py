"""Owned cache publications test export isolation, never actual inference."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from native_model_export import export
from native_model_inputs import InputError,read_json,verify
from test_native_model_cache import Fixture


class ExportFixture:
  def __init__(self,base):
    self.cache=base/'cache'; self.cache.mkdir(); self.store=base/'exports'
    self.builders={}
    for kind in ('driving','driver'):
      f=Fixture(base); f.root=self.cache/(kind+'-models')
      for c in f.contracts: c['kind']=kind
      self.builders[kind]=f
    self.results={k:f.publish() for k,f in self.builders.items()}

  def run(self,validate=None,revalidate=lambda:None):
    requests={k:[(c,65536,None) for c in f.contracts] for k,f in self.builders.items()}
    def check(root):
      assert not (root/'.model-cache').exists() and not (root/'lock').exists()
      for kind,f in self.builders.items(): f.validate(root/(kind+'-models'))
      return dict(gpu_executed=False,runtime_activated=False)
    return export(self.cache,self.store,self.results,requests,validate or check,revalidate)


class ModelExportTests(unittest.TestCase):
  def test_export_contains_only_selected_catalogs_and_reuses_all_data_on_repeat(self):
    with tempfile.TemporaryDirectory() as temp:
      f=ExportFixture(Path(temp)); first=f.run(); second=f.run()
      self.assertEqual(first['identity'],second['identity'])
      self.assertEqual(first['tree_sha256'],second['tree_sha256'])
      self.assertEqual(second['copied_bytes'],0); self.assertEqual(second['linked_bytes'],0)
      self.assertGreater(first['copied_bytes'],0); self.assertGreater(first['linked_bytes'],0)
      root=Path(first['root']); verify(read_json(Path(first['tree_receipt']),16*1024**2),root)
      for path in first['files']:
        self.assertTrue(path.startswith('rust/bin/'))
        self.assertNotIn('.model-cache',path)
      kind='driving'; generation=f.results[kind]['generation']; name='driving-1928x1208/weights.bin'
      self.assertEqual((root/f'{kind}-models'/generation/name).stat().st_ino,
                       (f.cache/f'{kind}-models'/generation/name).stat().st_ino)
      self.assertEqual(len(first['runtime_inputs']['trees']),1)

  def test_current_pointer_can_change_without_changing_an_old_export(self):
    with tempfile.TemporaryDirectory() as temp:
      f=ExportFixture(Path(temp)); first=f.run(); root=Path(first['root'])
      pointer=(root/'driver-models/current.json').read_bytes()
      for c in f.builders['driver'].contracts: c['model']['sha256']='f'*64
      f.results['driver']=f.builders['driver'].publish()
      second=f.run()
      self.assertNotEqual(first['root'],second['root'])
      self.assertEqual((root/'driver-models/current.json').read_bytes(),pointer)
      verify(read_json(Path(first['tree_receipt']),16*1024**2),root)

  def test_corrupt_export_is_rejected_without_healing_from_valid_cache(self):
    with tempfile.TemporaryDirectory() as temp:
      f=ExportFixture(Path(temp)); first=f.run(); path=Path(first['root'])/'driver-models/current.json'
      path.chmod(0o644); path.write_bytes(b'corrupt immutable export')
      with self.assertRaises(InputError): f.run()
      self.assertEqual(path.read_bytes(),b'corrupt immutable export')

  def test_failed_export_validation_never_publishes_new_generation(self):
    with tempfile.TemporaryDirectory() as temp:
      f=ExportFixture(Path(temp)); first=f.run(); root=Path(first['root'])
      for c in f.builders['driver'].contracts: c['model']['sha256']='f'*64
      f.results['driver']=f.builders['driver'].publish()
      def failure(_): raise InputError('owned export validation failure')
      with self.assertRaises(InputError): f.run(validate=failure)
      verify(read_json(Path(first['tree_receipt']),16*1024**2),root)
      self.assertEqual(len(list(f.store.glob('*/generation'))),1)


if __name__=='__main__': unittest.main()

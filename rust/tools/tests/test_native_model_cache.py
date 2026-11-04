"""Owned publication fixtures: never inference or QCOM compilation evidence."""
import copy
import fcntl
from pathlib import Path
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_model_cache as cache
from native_model_inputs import InputError, encoded, read_json, sha


ORIGIN = dict(commit='1'*40, tree='2'*40)


class Fixture:
  def __init__(self, base):
    self.root = base/'catalog'
    self.calls, self.validations = [], 0
    self.contracts = [dict(schema=1, kind='driver', camera=camera,
        model=dict(sha256='a'*64, bytes=17), nv12=dict(stride=1, y_height=2, uv_height=3, bytes=4),
        common=dict(producer='b'*64, python='c'*64, wheels='d'*64, compiler='e'*64, target='aarch64'))
        for camera in ([1928,1208], [1344,760])]

  def producer(self, contract, destination):
    self.calls.append(copy.deepcopy(contract))
    destination.mkdir()
    weights = encoded(contract)
    (destination/'weights.bin').write_bytes(weights)
    (destination/'graph.json').write_bytes(encoded(dict(owned_fixture=True, weights_sha256=sha(weights))))
    (destination/'pipeline.json').write_bytes(encoded(dict(kind=contract['kind'], camera=contract['camera'],
        nv12=contract['nv12'], sources=dict(onnx=contract['model']['sha256']))))

  def validate(self, root):
    self.validations += 1
    pointer = read_json(root/'current.json')
    generation = root/pointer['generation']
    assert sha((generation/'index.json').read_bytes()) == pointer['generation']
    for entry in read_json(generation/'index.json')['bundles']:
      bundle = generation/entry['directory']
      assert read_json(bundle/'graph.json')['weights_sha256'] == sha((bundle/'weights.bin').read_bytes())

  def publish(self, *, origin=ORIGIN, validate=None, revalidate=lambda: None, bound=65536):
    requests = [(c, bound, lambda path, c=c: self.producer(c, path)) for c in self.contracts]
    return cache.publish(self.root, requests, origin, validate or self.validate, revalidate)

  def current(self): return (self.root/'current.json').read_bytes()


class GeneratedModelTests(unittest.TestCase):
  def test_repeat_skips_producer_preserves_original_provenance_and_inode(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      first = f.publish()
      generation = f.root/first['generation']
      weights = generation/'driver-1928x1208/weights.bin'
      inode, pointer = weights.stat().st_ino, f.current()
      with mock.patch.object(f, 'producer', side_effect=AssertionError('warm producer called')):
        second = f.publish(origin=dict(commit='3'*40, tree='4'*40),bound=131072)
      self.assertEqual(first['generated_pipelines'], 2)
      self.assertEqual(second['generated_pipelines'], 0)
      self.assertEqual(second['reused_pipelines'], 2)
      self.assertEqual(second['linked_output_bytes'], 0)
      self.assertEqual(second['origins'], [ORIGIN, ORIGIN])
      self.assertEqual(second['generation'], first['generation'])
      self.assertEqual((weights.stat().st_ino, f.current()), (inode, pointer))
      self.assertEqual(f.validations, 2)

  def test_only_affected_pipeline_rebuilds_and_previous_generation_survives(self):
    with tempfile.TemporaryDirectory() as temp:
      f = Fixture(Path(temp))
      first = f.publish()
      old = f.root/first['generation']
      old_weights = old/'driver-1344x760/weights.bin'
      inode = old_weights.stat().st_ino
      f.contracts[0]['model']['sha256'] = 'f'*64
      second = f.publish()
      self.assertEqual((second['generated_pipelines'],second['reused_pipelines']), (1,1))
      self.assertNotEqual(first['generation'], second['generation'])
      self.assertTrue(old.is_dir())
      self.assertEqual((f.root/second['generation']/'driver-1344x760/weights.bin').stat().st_ino,inode)
      self.assertEqual(len(f.calls),3)

  def test_every_generation_input_changes_its_identity(self):
    for field in ('producer','python','wheels','compiler','target'):
      with self.subTest(field=field), tempfile.TemporaryDirectory() as temp:
        f = Fixture(Path(temp))
        first=f.publish()
        for c in f.contracts: c['common'][field] = 'changed-input'
        second=f.publish()
        self.assertEqual(second['generated_pipelines'],2)
        self.assertTrue(set(first['inputs']).isdisjoint(second['inputs']))

  def test_corrupt_generated_cache_fails_without_regeneration_or_pointer_change(self):
    for case in ('weights','extra','mode','symlink','receipt','missing'):
      with self.subTest(case=case), tempfile.TemporaryDirectory() as temp:
        f=Fixture(Path(temp)); first=f.publish(); pointer=f.current()
        result=next((f.root/'.model-cache'/first['inputs'][0]).iterdir())
        path=result/'bundle/weights.bin'
        if case=='weights': path.chmod(0o644); path.write_bytes(b'corrupt'); path.chmod(0o444)
        if case=='extra': (result/'bundle/extra').write_bytes(b'extra')
        if case=='mode': path.chmod(0o644)
        if case=='symlink': path.unlink(); path.symlink_to('/etc/passwd')
        if case=='missing': path.unlink()
        if case=='receipt':
          receipt=result/'receipt.json'; receipt.chmod(0o644); receipt.write_bytes(b'{}')
        with mock.patch.object(f,'producer',side_effect=AssertionError('corruption regenerated')):
          with self.assertRaises((InputError,FileNotFoundError)): f.publish()
        self.assertEqual(f.current(),pointer)

  def test_rehashed_receipt_for_different_source_is_still_rejected(self):
    with tempfile.TemporaryDirectory() as temp:
      f=Fixture(Path(temp)); first=f.publish(); pointer=f.current()
      result=next((f.root/'.model-cache'/first['inputs'][0]).iterdir())
      value=read_json(result/'receipt.json')
      value['input']['model']['sha256']='f'*64
      raw=encoded(value); receipt=result/'receipt.json'
      receipt.chmod(0o644); receipt.write_bytes(raw); receipt.chmod(0o444)
      result.rename(result.parent/sha(raw))
      with self.assertRaisesRegex(InputError,'requested inputs'): f.publish()
      self.assertEqual(f.current(),pointer)

  def test_producer_consumer_input_change_and_interrupt_preserve_current_and_logs(self):
    for case in ('producer','consumer','input-change','interrupt','oversize'):
      with self.subTest(case=case), tempfile.TemporaryDirectory() as temp:
        f=Fixture(Path(temp)); f.publish(); pointer=f.current()
        log=f.root/'existing.log'; log.write_bytes(b'keep log')
        orphan=f.root/'.build-other-owner'; orphan.mkdir(); (orphan/'keep').write_bytes(b'keep pending')
        f.contracts[0]['model']['sha256']='f'*64
        if case=='oversize': f.contracts[1]['model']['sha256']='f'*64
        original=f.producer
        def producer(contract,path):
          if case=='producer': raise InputError('owned compiler failure')
          if case=='interrupt': raise KeyboardInterrupt()
          original(contract,path)
        def failure(*_): raise InputError('owned validation failure')
        with mock.patch.object(f,'producer',side_effect=producer):
          with self.assertRaises((InputError,KeyboardInterrupt)):
            f.publish(validate=failure if case=='consumer' else None,
                      revalidate=failure if case=='input-change' else lambda: None,
                      bound=1 if case=='oversize' else 65536)
        self.assertEqual(f.current(),pointer)
        self.assertEqual(log.read_bytes(),b'keep log')
        self.assertEqual((orphan/'keep').read_bytes(),b'keep pending')
        self.assertEqual(set(p.name for p in f.root.glob('.build-*')), {orphan.name})

  def test_busy_lock_and_low_space_preserve_current_and_allow_retry(self):
    with tempfile.TemporaryDirectory() as temp:
      f=Fixture(Path(temp)); f.publish(); pointer=f.current()
      with (f.root/'lock').open('ab') as stream:
        fcntl.flock(stream,fcntl.LOCK_EX|fcntl.LOCK_NB)
        with self.assertRaises(BlockingIOError): f.publish()
      f.contracts[0]['model']['sha256']='f'*64
      with mock.patch.object(cache.shutil,'disk_usage',return_value=SimpleNamespace(free=0)):
        with self.assertRaisesRegex(InputError,'capacity'): f.publish()
      self.assertEqual(f.current(),pointer)
      self.assertEqual(f.publish()['generated_pipelines'],1)

  def test_catalog_corruption_and_ambiguous_cache_fail_closed(self):
    for case in ('index','ambiguous','cache-link','catalog-link','pointer'):
      with self.subTest(case=case), tempfile.TemporaryDirectory() as temp:
        f=Fixture(Path(temp)); first=f.publish(); pointer=f.current()
        parent=f.root/'.model-cache'/first['inputs'][0]
        if case=='index':
          index=f.root/first['generation']/'index.json'; index.chmod(0o644); index.write_bytes(b'{}')
        elif case=='ambiguous': (parent/'unexpected').mkdir()
        elif case=='cache-link':
          target=Path(temp)/'moved-cache'; parent.rename(target); parent.symlink_to(target,target_is_directory=True)
        elif case=='catalog-link':
          target=Path(temp)/'moved-catalog'; (f.root/first['generation']).rename(target)
          (f.root/first['generation']).symlink_to(target,target_is_directory=True)
        else:
          (f.root/'current.json').chmod(0o644)
          (f.root/'current.json').write_bytes(encoded(dict(version=1,generation='../escape')))
          pointer=f.current()
        with self.assertRaises(InputError): f.publish()
        self.assertEqual(f.current(),pointer)

  def test_duplicate_selection_and_bad_descriptor_never_publish_pointer(self):
    with tempfile.TemporaryDirectory() as temp:
      f=Fixture(Path(temp)); f.contracts[1]=copy.deepcopy(f.contracts[0])
      with self.assertRaisesRegex(InputError,'duplicate'): f.publish()
      self.assertFalse(f.root.exists())
    with tempfile.TemporaryDirectory() as temp:
      f=Fixture(Path(temp)); original=f.producer
      def producer(contract,path):
        original(contract,path)
        data=read_json(path/'pipeline.json'); data['sources']['onnx']='f'*64
        (path/'pipeline.json').write_bytes(encoded(data))
      with mock.patch.object(f,'producer',side_effect=producer):
        with self.assertRaisesRegex(InputError,'descriptor'): f.publish()
      self.assertFalse((f.root/'current.json').exists())


if __name__=='__main__':
  unittest.main()

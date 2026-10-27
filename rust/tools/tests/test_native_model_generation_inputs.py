"""Owned Git trees exercise input selection, never real model generation."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import native_model_generation as generation
from native_model_inputs import InputError, sha, source_identity


class Source:
  def __init__(self, path):
    self.path=path
    self.git('init','-q')
    self.git('config','user.name','Owned fixture')
    self.git('config','user.email','fixture@example.invalid')
    for name in generation.SOURCE_FILES: self.put(name,b'# owned input\n')
    for name in generation.SOURCE_ROOTS: self.put(name+'/owned.py',b'# owned producer\n')
    self.put(generation.MODELS['driving'],b'version https://git-lfs.github.com/spec/v1\noid sha256:'+b'a'*64+b'\nsize 17\n')
    self.put(generation.MODELS['driver'],b'owned ordinary model bytes')
    self.put('.gitignore',b'*.ignored.py\n')
    self.commit()

  def git(self,*args):
    return subprocess.check_output(['git','-C',self.path,*args],stderr=subprocess.STDOUT)

  def put(self,name,value):
    p=self.path/name; p.parent.mkdir(parents=True,exist_ok=True); p.write_bytes(value)

  def commit(self):
    self.git('add','.'); self.git('-c','commit.gpgsign=false','-c','core.hooksPath=/dev/null','commit','-qm','owned fixture')


class GenerationInputTests(unittest.TestCase):
  def test_docs_and_unrelated_rust_keep_code_identity_but_producer_additions_invalidate(self):
    with tempfile.TemporaryDirectory() as temp:
      s=Source(Path(temp)); before=generation.source_closure(s.path); origin=source_identity(s.path)
      s.put('docs/owned.md',b'owned docs change'); s.put('rust/crates/loggerd/owned.rs',b'// owned Rust change\n'); s.commit()
      self.assertNotEqual(origin,source_identity(s.path))
      self.assertEqual(before,generation.source_closure(s.path))
      s.put('tinygrad_repo/tinygrad/new_module.py',b'# added producer input\n'); s.commit()
      self.assertNotEqual(before,generation.source_closure(s.path))

  def test_ignored_python_shadow_and_missing_tracked_input_are_rejected(self):
    with tempfile.TemporaryDirectory() as temp:
      s=Source(Path(temp)); p=s.path/'tinygrad_repo/tinygrad/owned.ignored.py'; p.write_bytes(b'# ignored shadow\n')
      source_identity(s.path)
      with self.assertRaisesRegex(InputError,'untracked'): generation.source_closure(s.path)
      p.unlink(); (s.path/generation.SOURCE_FILES[0]).unlink()
      with self.assertRaises(FileNotFoundError): generation.source_closure(s.path)

  def test_current_git_lfs_and_ordinary_model_bytes_are_separate_from_request_commit(self):
    with tempfile.TemporaryDirectory() as temp:
      s=Source(Path(temp)); commit=source_identity(s.path)['commit']
      models=generation.model_inputs(s.path,commit)
      self.assertTrue(models['driving']['lfs']); self.assertEqual(models['driving']['bytes'],17)
      self.assertIn('/'+commit+'/',models['driving']['url'])
      self.assertFalse(models['driver']['lfs'])
      self.assertEqual(models['driver']['sha256'],sha(b'owned ordinary model bytes'))
      changed=generation.model_inputs(s.path,'b'*40)
      for kind in models:
        self.assertEqual({k:v for k,v in models[kind].items() if k!='url'},
                         {k:v for k,v in changed[kind].items() if k!='url'})
      (s.path/generation.MODELS['driver']).write_bytes(b'changed working tree')
      with self.assertRaises(InputError): generation.model_inputs(s.path,commit)

  def test_malformed_current_git_lfs_pointer_is_rejected(self):
    with tempfile.TemporaryDirectory() as temp:
      s=Source(Path(temp))
      s.put(generation.MODELS['driving'],b'version https://git-lfs.github.com/spec/v1\noid sha256:untrusted\nsize 17\n'); s.commit()
      with self.assertRaisesRegex(InputError,'LFS pointer'): generation.model_inputs(s.path,source_identity(s.path)['commit'])


if __name__=='__main__':
  unittest.main()

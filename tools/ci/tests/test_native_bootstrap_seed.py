"""Owned ELF-shaped fixtures are never executed or described as native proof."""
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

SOURCE = Path(__file__).resolve().parents[2]/'native/bootstrap_seed.py'
SPEC = importlib.util.spec_from_file_location('native_bootstrap_seed', SOURCE)
seed = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(seed)


class Fixture:
  def __init__(self, root):
    self.root, self.cache = root, root/'cache'
    self.calls = []
    header = bytearray(80)
    header[:7] = b'\x7fELF\x02\x01\x01'
    header[18:20] = (183).to_bytes(2, 'little')
    header[20:24] = (1).to_bytes(4, 'little')
    header[52:54] = (64).to_bytes(2, 'little')
    header[79] = 42
    self.bodies = {'helper': bytes(header), 'notices': b'Owned fixture license\n'}
    pin = dict(resolved='/usr/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1', bytes=1, mode=0o755, sha256='a'*64)
    commit = 'b'*40
    base = 'https://github.com/bin9208/openpilot-rust/releases/download/native-bootstrap-'+commit+'/'
    self.plan = dict(schema=1, role=seed.ROLE, source=dict(commit=commit, tree='c'*40), target='aarch64-unknown-linux-gnu', agnos=seed.AGNOS,
        assets={k:dict(url=base+seed.FILES[k], bytes=len(v), sha256=hashlib.sha256(v).hexdigest()) for k,v in self.bodies.items()},
        abi=dict(interpreter='/lib/ld-linux-aarch64.so.1',libraries={'/lib/ld-linux-aarch64.so.1':pin},
                 tools={'/usr/bin/ldd':pin,'/usr/bin/readelf':pin}))
    self.plan['build'] = {key:'d'*64 for key in ('cargo_lock_sha256','cargo_inputs_plan_sha256','cargo_inputs_tree_sha256','rust_inputs_plan_sha256','rustc_sha256','cargo_sha256')}
    self.plan['build']['features'] = ['prepare-rust','prepare-cargo','prepare-capnp']
    self.manifest = root/'manifest.json'
    self.save()

  def save(self):
    data = seed.json_bytes(self.plan)
    self.manifest.write_bytes(data)
    self.sha = hashlib.sha256(data).hexdigest()

  def download(self, asset, path):
    name = next(k for k,v in self.plan['assets'].items() if v == asset)
    self.calls.append(name)
    path.write_bytes(self.bodies[name])
    return len(self.bodies[name])

  def prepare(self):
    return seed.prepare(self.manifest, self.sha, self.cache, download=self.download)[0]


class BootstrapSeedTests(unittest.TestCase):
  def test_legacy_seed_requires_an_independently_pinned_owner_chain_protocol_for_intermediaries(self):
    with tempfile.TemporaryDirectory() as temporary:
      fixture = Fixture(Path(temporary))
      external = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
      try:
        original = seed.process_identity(external.pid)
        runner = mock.Mock()
        with mock.patch.object(seed, 'verify_native'), mock.patch.object(os, 'geteuid', return_value=1000):
          with self.assertRaisesRegex(ValueError, 'owner chain'):
            seed.current_tools(dict(root=str(fixture.root)), fixture.plan,
                fixture.root/'unused.json', fixture.root, request_owner=original, run=runner)
        runner.assert_not_called()
        fixture.plan['protocols'] = dict(current_tools=1, request_owner=2)
        fixture.save()
        self.assertEqual(seed.read_plan(fixture.manifest, fixture.sha)[0]['protocols']['request_owner'], 2)
        fixture.plan['protocols']['generation_boot'] = 1
        fixture.save()
        self.assertEqual(seed.read_plan(fixture.manifest, fixture.sha)[0]['protocols']['generation_boot'], 1)
        fixture.plan['protocols']['generation_boot'] = True
        fixture.save()
        with self.assertRaisesRegex(ValueError, 'protocol'):
          seed.read_plan(fixture.manifest, fixture.sha)
        fixture.plan['protocols']['generation_boot'] = 1
        fixture.plan['protocols']['current_tools'] = True
        fixture.save()
        with self.assertRaisesRegex(ValueError, 'protocol'):
          seed.read_plan(fixture.manifest, fixture.sha)
      finally:
        external.kill()
        external.wait()

  def test_fixed_seed_reuses_without_transfer_or_project_commit_input(self):
    with tempfile.TemporaryDirectory() as temporary:
      fixture = Fixture(Path(temporary))
      first = fixture.prepare()
      self.assertEqual(fixture.calls, ['helper','notices'])
      self.assertGreater(first['stored_bytes'],0)
      self.assertFalse(first['seed_executed'] or first['compiler_executed'] or first['runtime_activated'])
      # Different consumer checkouts do not change the pinned seed's identity.
      (fixture.root/'consumer-commit').write_text('different current project commit')
      repeat = fixture.prepare()
      self.assertEqual(repeat['root'],first['root'])
      self.assertEqual(repeat['seed_source'],fixture.plan['source'])
      self.assertEqual(repeat['stored_bytes'],0)
      self.assertEqual(repeat['transferred_bytes'],dict(helper=0,notices=0))
      self.assertEqual(len(fixture.calls),2)
      self.assertEqual(stat.S_IMODE(Path(first['helper']).stat().st_mode),0o555)

  def test_modified_seed_and_rewritten_local_descriptor_do_not_certify_themselves(self):
    with tempfile.TemporaryDirectory() as temporary:
      fixture = Fixture(Path(temporary))
      prepared = fixture.prepare()
      normal = fixture.root/'normal-runtime-and-logs'
      normal.write_text('preserved')
      helper = Path(prepared['helper'])
      helper.chmod(0o644)
      damaged = bytearray(helper.read_bytes()); damaged[-1] ^= 1
      helper.write_bytes(damaged); helper.chmod(0o555)
      forged = json.loads(json.dumps(fixture.plan))
      forged['assets']['helper']['sha256'] = hashlib.sha256(damaged).hexdigest()
      local = helper.parent/'manifest.json'
      local.chmod(0o644); local.write_bytes(seed.json_bytes(forged)); local.chmod(0o444)
      with self.assertRaises(ValueError): fixture.prepare()
      self.assertEqual(normal.read_text(),'preserved')
      self.assertEqual(len(fixture.calls),2)

  def test_bad_pin_role_size_body_or_elf_never_publishes(self):
    for fault in ('descriptor','role','source','build-inputs','asset-url','size','body','elf'):
      with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
        fixture = Fixture(Path(temporary))
        if fault == 'role': fixture.plan['role'] = 'openpilot-manager'
        if fault == 'source': fixture.plan['source']['commit'] = 'short'
        if fault == 'build-inputs': fixture.plan['build']['features'] = []
        if fault == 'asset-url': fixture.plan['assets']['helper']['url'] = 'https://other.invalid/program'
        if fault == 'size': fixture.plan['assets']['helper']['bytes'] = 64*1024**2+1
        if fault == 'body': fixture.bodies['helper'] = b'changed'
        if fault == 'elf':
          fixture.bodies['helper'] = b'x'*80
          fixture.plan['assets']['helper']['sha256'] = hashlib.sha256(fixture.bodies['helper']).hexdigest()
        fixture.save()
        if fault == 'descriptor': fixture.sha = '0'*64
        with self.assertRaises(ValueError): fixture.prepare()
        self.assertFalse((fixture.cache/fixture.sha/'generation').exists())

  def test_low_space_busy_owner_and_unsafe_cached_members_preserve_valid_seed(self):
    with tempfile.TemporaryDirectory() as temporary:
      fixture = Fixture(Path(temporary))
      with mock.patch.object(seed.shutil,'disk_usage',return_value=SimpleNamespace(free=0)):
        with self.assertRaises(ValueError): fixture.prepare()
      self.assertFalse(fixture.calls)
      first = fixture.prepare()
      abandoned = Path(first['root']).parent/'.pending-interrupted'
      abandoned.mkdir(); (abandoned/'partial').write_text('unpublished bytes')
      unrelated = Path(first['root']).parent/'other-owner-data'
      unrelated.mkdir(); (unrelated/'keep').write_text('preserved')
      with mock.patch.object(seed.shutil,'disk_usage',return_value=SimpleNamespace(free=0)):
        self.assertEqual(fixture.prepare()['stored_bytes'],0)
      self.assertFalse(abandoned.exists())
      self.assertEqual((unrelated/'keep').read_text(),'preserved')
      with (Path(first['root']).parent/'lock').open('wb') as owner:
        fcntl.flock(owner,fcntl.LOCK_EX | fcntl.LOCK_NB)
        with self.assertRaises(BlockingIOError): fixture.prepare()
      (Path(first['root'])/'unexpected').write_text('not in the published generation')
      with self.assertRaises(ValueError): fixture.prepare()
      (Path(first['root'])/'unexpected').unlink()
      helper = Path(first['helper'])
      saved = helper.read_bytes(); helper.unlink(); helper.symlink_to('/bin/true')
      with self.assertRaises(OSError): fixture.prepare()
      helper.unlink(); helper.write_bytes(saved); helper.chmod(0o555)
      self.assertEqual(fixture.prepare()['stored_bytes'],0)


if __name__ == '__main__':
  unittest.main()

"""Owned ELF-shaped fixtures are never executed or described as native proof."""
import fcntl
import copy
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
PROOT_SPEC = importlib.util.spec_from_file_location('owned_native_proot',
    Path(__file__).resolve().parents[1]/'native_proot.py')
proot = importlib.util.module_from_spec(PROOT_SPEC)
PROOT_SPEC.loader.exec_module(proot)


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
  def test_abi_uses_the_declared_loader_and_fixed_environment_with_compatible_pins(self):
    binary = Path('owned-seed').absolute()
    interpreter = '/lib/ld-linux-aarch64.so.1'
    libc = '/lib/aarch64-linux-gnu/libc.so.6'
    program = f'[Requesting program interpreter: {interpreter}]\n'.encode()
    mapping = f'linux-vdso.so.1 (0x1)\nlibc.so.6 => {libc} (0x2)\n{interpreter} (0x3)\n'.encode()
    pin = dict(resolved=interpreter, bytes=1, mode=0o755, sha256='a'*64)
    with mock.patch.dict(os.environ, LD_LIBRARY_PATH='/untrusted'), \
         mock.patch.object(seed.subprocess, 'run', side_effect=[
             subprocess.CompletedProcess([], 0, program, b''),
             subprocess.CompletedProcess([], 0, mapping, b'')]) as runner, \
         mock.patch.object(seed, 'file_pin', return_value=pin) as file_pin:
      actual = seed.inspect_abi(Path('owned-seed'))
    options = dict(env={'PATH':'/usr/bin:/bin','LC_ALL':'C'}, stdout=subprocess.PIPE,
                   stderr=subprocess.PIPE, timeout=30, check=True)
    self.assertEqual(runner.call_args_list, [
        mock.call(['/usr/bin/readelf','--wide','-l',str(binary)], **options),
        mock.call([interpreter,'--list',str(binary)], **options)])
    self.assertEqual(actual, dict(interpreter=interpreter, libraries={libc:pin,interpreter:pin},
        tools={'/usr/bin/readelf':pin,'/usr/bin/ldd':pin}))
    self.assertEqual(file_pin.call_args_list, [mock.call(libc),mock.call(interpreter),
        mock.call('/usr/bin/readelf'),mock.call('/usr/bin/ldd')])

  def test_abi_still_rejects_other_interpreters_and_invalid_loader_mappings(self):
    interpreter = '/lib/ld-linux-aarch64.so.1'
    valid = f'[Requesting program interpreter: {interpreter}]\n'.encode()
    pin = dict(resolved=interpreter, bytes=1, mode=0o755, sha256='a'*64)
    for program in (b'no interpreter\n', b'[Requesting program interpreter: /other/loader]\n'):
      with self.subTest(program=program), mock.patch.object(seed.subprocess, 'run',
          return_value=subprocess.CompletedProcess([], 0, program, b'')) as runner:
        with self.assertRaisesRegex(ValueError, 'unexpected native seed interpreter'):
          seed.inspect_abi(Path('/owned-seed'))
        self.assertEqual(runner.call_count, 1)
    for mapping, error in [
        ('libc.so.6 => not found\n','unresolved or unsupported'),
        (f'/outside/libc.so.6 (0x1)\n{interpreter} (0x2)\n','unexpected native seed provider path'),
        (f'{interpreter} (0x1)\n{interpreter} (0x2)\n','unexpected native seed provider path'),
        ('/lib/aarch64-linux-gnu/libc.so.6 (0x1)\n','interpreter was not resolved')]:
      with self.subTest(mapping=mapping), mock.patch.object(seed.subprocess, 'run', side_effect=[
          subprocess.CompletedProcess([], 0, valid, b''),
          subprocess.CompletedProcess([], 0, mapping.encode(), b'')]), \
          mock.patch.object(seed, 'file_pin', return_value=pin):
        with self.assertRaisesRegex(ValueError, error):
          seed.inspect_abi(Path('/owned-seed'))

  def test_abi_command_failure_retains_argv_status_and_bounded_streams(self):
    binary = Path('/owned-seed')
    interpreter = '/lib/ld-linux-aarch64.so.1'
    argv = [interpreter,'--list',str(binary)]
    program = f'[Requesting program interpreter: {interpreter}]\n'.encode()
    for stdout, stderr in ((b'',b''), (b'loader stdout\n'+b'o'*9000,b'loader stderr\n'+b'e'*9000)):
      failure = subprocess.CalledProcessError(1, argv, output=stdout, stderr=stderr)
      with self.subTest(stdout_bytes=len(stdout),stderr_bytes=len(stderr)), \
           mock.patch.object(seed.subprocess, 'run', side_effect=[
               subprocess.CompletedProcess([], 0, program, b''),failure]), \
           mock.patch.object(seed, 'file_pin') as file_pin:
        with self.assertRaisesRegex(ValueError, 'seed ABI inspection command failed') as caught:
          seed.inspect_abi(binary)
        file_pin.assert_not_called()
      self.assertIs(caught.exception.__cause__, failure)
      details = json.loads(str(caught.exception).split(': ',1)[1])
      self.assertEqual(details['argv'], argv)
      self.assertEqual(details['returncode'], 1)
      for name, content in (('stdout',stdout),('stderr',stderr)):
        self.assertEqual(details[name], content[:8192].decode())
        self.assertEqual(details[name+'_bytes'], len(content))
        self.assertEqual(details[name+'_truncated'], len(content)>8192)

  def test_abi_combined_output_bound_applies_to_success_and_failure(self):
    interpreter = '/lib/ld-linux-aarch64.so.1'
    argv = [interpreter,'--list','/owned-seed']
    program = f'[Requesting program interpreter: {interpreter}]\n'.encode()
    mapping = f'{interpreter} (0x1)\n'.encode()
    pin = dict(resolved=interpreter, bytes=1, mode=0o755, sha256='a'*64)
    for extra, failed in ((0,False),(1,False),(1,True)):
      stderr = b'e'*(1024**2-len(mapping)+extra)
      result = (subprocess.CalledProcessError(1, argv, output=mapping, stderr=stderr) if failed
                else subprocess.CompletedProcess(argv, 0, mapping, stderr))
      with self.subTest(extra=extra,failed=failed), \
           mock.patch.object(seed.subprocess, 'run', side_effect=[
               subprocess.CompletedProcess([], 0, program, b''),result]), \
           mock.patch.object(seed, 'file_pin', return_value=pin):
        if not extra:
          self.assertEqual(seed.inspect_abi(Path('/owned-seed'))['interpreter'], interpreter)
        else:
          with self.assertRaisesRegex(ValueError, 'exceeded its output bound') as caught:
            seed.inspect_abi(Path('/owned-seed'))
          details = json.loads(str(caught.exception).split(': ',1)[1])
          self.assertEqual(details['returncode'], 1 if failed else 0)
          self.assertEqual(details['stdout_bytes']+details['stderr_bytes'], 1024**2+1)
          self.assertEqual(len(details['stderr']), 8192)
          self.assertTrue(details['stderr_truncated'])

  def test_owner_identity_matches_an_independent_kernel_observer(self):
    pid = os.getpid()
    identity = seed.process_identity(pid)
    observer = ('from pathlib import Path; import sys; pid=int(sys.argv[1]); '
                'fields=Path(f"/proc/{pid}/task/{pid}/stat").read_text().rsplit(") ",1)[1].split(); '
                'print(f"{pid}:{int(fields[19])}")')
    observed = subprocess.check_output(['/usr/bin/python3', '-I', '-B', '-S', '-c', observer, str(pid)],
        env=seed.ENV, text=True, timeout=10).strip()
    self.assertEqual(identity, observed)
    seed.verify_owners(identity)
    with self.assertRaisesRegex(ValueError, 'identity changed'):
      seed.verify_owners(f'{pid}:{int(identity.split(":")[1])+1}')

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


class NativeProotPolicyTests(unittest.TestCase):
  """ELF-shaped bytes and source-pin stubs; no native tool build or execution."""

  def elf(self, kind=1, machine=183):
    data = bytearray(128)
    data[:7] = b'\x7fELF\x02\x01\x01'
    data[18:20] = machine.to_bytes(2, 'little')
    data[32:40] = (64).to_bytes(8, 'little')
    data[54:56] = (56).to_bytes(2, 'little')
    data[56:58] = (1).to_bytes(2, 'little')
    data[64:68] = kind.to_bytes(4, 'little')
    if kind == 2:
      data[72:80] = (128).to_bytes(8, 'little')
      data[96:104] = (32).to_bytes(8, 'little')
      data += (1).to_bytes(8, 'little')+bytes(24)  # DT_NEEDED followed by DT_NULL.
    return data

  def fixture(self, work):
    source = work.parent/'source'
    source.mkdir()
    with mock.patch.dict(os.environ, CI_JOB_TEMP=str(work.parent), GITHUB_WORKSPACE=str(source)):
      guest = proot.expected_guest_command(work)
    binary = work/'proot'
    binary.write_bytes(self.elf())
    binary.chmod(0o555)
    talloc_pin = dict(bytes=1, sha256='a'*64)
    proof = dict(schema=1, source_commit='b'*40, execution_mode=proot.MODE, host_architecture='aarch64',
        sources=copy.deepcopy(proot.SOURCES), build_affinity=[2, 4], build_argv=proot.build_argv(work, [2, 4]),
        build_environment=proot.build_environment(work), build_exit_code=0,
        static_talloc=dict(path='/usr/lib/aarch64-linux-gnu/libtalloc.a', **talloc_pin),
        executable=dict(path=str(binary), **proot.static_arm_elf(binary)))
    receipt = work/'native-bootstrap-proot-build.json'
    receipt.write_text(json.dumps(proof))
    context = dict(schema=1, source_commit='b'*40, work=str(work), execution_mode=proot.MODE,
        guest_work=proot.GUEST_WORK, source_mount=proot.SOURCE_MOUNT,
        proot_source_sha256=proot.SOURCES['proot']['sha256'],
        uthash_source_sha256=proot.SOURCES['uthash']['sha256'],
        proot_sha256=proof['executable']['sha256'], proot_build_receipt_sha256=proot.file_pin(receipt)['sha256'],
        guest=guest)
    pins = {str(work/(name+'-source.tar.gz')): {k: pin[k] for k in ('bytes', 'sha256')}
            for name, pin in proot.SOURCES.items()}
    pins['/usr/lib/aarch64-linux-gnu/libtalloc.a'] = talloc_pin
    original_pin = proot.file_pin
    def file_pin(path):
      return pins[str(path)] if str(path) in pins else original_pin(path)
    return context, proof, file_pin, pins

  def test_static_arm_checker_rejects_other_cpu_interpreter_and_dynamic_dependencies(self):
    with tempfile.TemporaryDirectory() as temporary:
      path = Path(temporary)/'owned-elf'
      for data in (self.elf(machine=62), self.elf(kind=3), self.elf(kind=2)):
        path.write_bytes(data)
        path.chmod(0o555)
        with self.assertRaises(ValueError):
          proot.static_arm_elf(path)
        path.chmod(0o755)
      path.write_bytes(self.elf())
      path.chmod(0o555)
      self.assertTrue(proot.static_arm_elf(path)['static'])
      path.chmod(0o755)
      with self.assertRaisesRegex(ValueError, 'mode'):
        proot.static_arm_elf(path)

  def test_build_jobs_match_recorded_cpu_affinity_without_a_fixed_cap(self):
    argv = proot.build_argv(Path('/owned'), [0, 4, 9])
    self.assertIn('-j3', argv)
    for affinity in ([], [1, 1], [4, 1], [-1], [True], '0,1'):
      with self.assertRaises(ValueError):
        proot.build_argv(Path('/owned'), affinity)

  def test_context_binds_built_bytes_source_gitlink_and_build_command(self):
    with tempfile.TemporaryDirectory() as temporary:
      work = Path(temporary)/'native-agnos'
      work.mkdir()
      context, proof, file_pin, pins = self.fixture(work)
      with mock.patch.dict(os.environ, GITHUB_SHA='b'*40, CI_JOB_TEMP=str(work.parent),
                           GITHUB_WORKSPACE=str(work.parent/'source')), \
           mock.patch.object(proot.os, 'uname', return_value=SimpleNamespace(machine='aarch64')), \
           mock.patch.object(proot, 'file_pin', side_effect=file_pin):
        self.assertEqual(proot.verify_context(context, work), context['guest'])
        for key in ('proot_source_sha256', 'uthash_source_sha256', 'proot_sha256', 'proot_build_receipt_sha256'):
          with self.assertRaises(ValueError):
            proot.verify_context(dict(context, **{key: 'c'*64}), work)
        pin = pins[str(work/'uthash-source.tar.gz')]
        pins[str(work/'uthash-source.tar.gz')] = dict(pin, sha256='c'*64)
        with self.assertRaisesRegex(ValueError, 'source archive'):
          proot.verify_context(context, work)
        pins[str(work/'uthash-source.tar.gz')] = pin
        receipt = work/'native-bootstrap-proot-build.json'
        changed = dict(proof, build_environment=dict(proof['build_environment'], LDFLAGS=''))
        receipt.write_text(json.dumps(changed))
        changed_context = dict(context, proot_build_receipt_sha256=proot.file_pin(receipt)['sha256'])
        with self.assertRaisesRegex(ValueError, 'provenance'):
          proot.verify_context(changed_context, work)

  def test_guest_rejects_qemu_and_fake_credentials_but_keeps_env_clear(self):
    with tempfile.TemporaryDirectory() as temporary:
      work = Path(temporary)/'native-agnos'
      work.mkdir()
      context, _, file_pin, _ = self.fixture(work)
      with mock.patch.dict(os.environ, GITHUB_SHA='b'*40, CI_JOB_TEMP=str(work.parent),
                           GITHUB_WORKSPACE=str(work.parent/'source')), \
           mock.patch.object(proot.os, 'uname', return_value=SimpleNamespace(machine='aarch64')), \
           mock.patch.object(proot, 'file_pin', side_effect=file_pin):
        for options in (['-q', '/usr/bin/qemu-aarch64-static'], ['-0'], ['-i', '1000:1000'], ['-S', '/']):
          changed = dict(context, guest=[context['guest'][0], *options, *context['guest'][1:]])
          with self.assertRaisesRegex(ValueError, 'fixed bind/environment'):
            proot.verify_context(changed, work)
        actual = proot.verify_context(context, work)
        self.assertEqual(actual[actual.index('/usr/bin/env')+1], '-i')

  def test_guest_rejects_every_changed_bind_and_modified_env_tail(self):
    with tempfile.TemporaryDirectory() as temporary:
      work = Path(temporary)/'native-agnos'
      work.mkdir()
      context, _, file_pin, _ = self.fixture(work)
      original = context['guest']
      mutations = []
      for position, value in enumerate(original):
        if value == '-b':
          changed = original.copy()
          changed[position+1] = '/unrelated:/unexpected'
          mutations.append(changed)
      missing_bind = original.copy()
      position = missing_bind.index('-b')
      del missing_bind[position:position+2]
      mutations.append(missing_bind)
      missing_env_clear = original.copy()
      del missing_env_clear[missing_env_clear.index('/usr/bin/env')+1]
      mutations.append(missing_env_clear)
      mutations.append([*original, 'LD_LIBRARY_PATH=/unrelated'])
      for position, value in enumerate(original):
        if value.startswith(('PATH=', 'CARGO_HOME=', 'RUSTUP_HOME=', 'GIT_CONFIG_GLOBAL=')):
          changed = original.copy()
          changed[position] = value.split('=', 1)[0]+'=/unrelated'
          mutations.append(changed)
      with mock.patch.dict(os.environ, GITHUB_SHA='b'*40, CI_JOB_TEMP=str(work.parent),
                           GITHUB_WORKSPACE=str(work.parent/'source')), \
           mock.patch.object(proot.os, 'uname', return_value=SimpleNamespace(machine='aarch64')), \
           mock.patch.object(proot, 'file_pin', side_effect=file_pin):
        for guest in mutations:
          with self.subTest(guest=guest):
            with self.assertRaisesRegex(ValueError, 'fixed bind/environment'):
              proot.verify_context(dict(context, guest=guest), work)
        self.assertEqual(proot.verify_context(context, work), original)


if __name__ == '__main__':
  unittest.main()

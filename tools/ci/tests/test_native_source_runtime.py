"""Owned protocol/filesystem/process tests; no project runtime built or started."""
import copy
from pathlib import Path
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'native'))
import source_runtime as driver
seed = driver.seed


class SourceRuntimeTests(unittest.TestCase):
  def memory_fixture(self, root, available=3219333120):
    meminfo = root/'meminfo'
    meminfo.write_text(f'MemTotal: 4000000 kB\nMemAvailable: {available//1024} kB\n')
    controller = root/'controller'
    controller.mkdir()
    policy = dict(schema=1, memory_cgroup_parent=str(controller/'not-created-yet'),
        maximum_memory_bytes=3*1024**3, minimum_available_memory_bytes=128*1024**2,
        filesystems={str(root): dict(maximum_growth_bytes=1, minimum_free_bytes=1)})
    return policy, meminfo, controller

  def test_reported_device_memory_fits_a_smaller_enforced_cap_without_editing_policy(self):
    with tempfile.TemporaryDirectory() as temp:
      policy, meminfo, _ = self.memory_fixture(Path(temp))
      original = copy.deepcopy(policy)
      self.assertGreater(3221225472+134217728, 3219333120, 'original device failure must remain reproducible')
      fitted, report = driver.memory.fit(policy, meminfo)
      self.assertEqual(fitted['maximum_memory_bytes'], 2885681152)
      self.assertEqual(fitted['minimum_available_memory_bytes'], 134217728)
      self.assertLessEqual(fitted['maximum_memory_bytes']+fitted['minimum_available_memory_bytes'], 3219333120)
      self.assertGreaterEqual(report['startup_headroom_bytes'], 128*1024**2)
      self.assertEqual(report['host_available_bytes'], 3219333120)
      self.assertEqual(policy, original)
      meminfo.write_text('MemAvailable: 8388608 kB\n')
      unchanged, _ = driver.memory.fit(policy, meminfo)
      self.assertEqual(unchanged, policy)

  def test_memory_fit_respects_v1_v2_ancestors_and_reclaimable_cache(self):
    for version in (1, 2):
      with self.subTest(version=version), tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        policy, meminfo, controller = self.memory_fixture(root, 8*1024**3)
        names = ('memory.limit_in_bytes', 'memory.usage_in_bytes') if version == 1 else ('memory.max', 'memory.current')
        (controller/names[0]).write_text(str(4*1024**3))
        (controller/names[1]).write_text(str(2*1024**3))
        (controller/'memory.stat').write_text(f'inactive_file {512*1024**2}\ntotal_inactive_file {1024**3}\n')
        fitted, report = driver.memory.fit(policy, meminfo)
        self.assertEqual(fitted['maximum_memory_bytes'], 2952790016)
        self.assertEqual(report['available_bytes'], 3*1024**3)
        self.assertEqual(report['ancestors'][0]['path'], str(controller))
        child = controller/'not-created-yet'
        child.mkdir()
        if version == 2:
          (child/'memory.max').write_text('max\n')
          self.assertEqual(driver.memory.fit(policy, meminfo)[0], fitted)
        (controller/names[1]).write_text(str(4*1024**3))
        (controller/'memory.stat').write_text('inactive_file 0\n')
        with self.assertRaisesRegex(ValueError, 'insufficient memory'):
          driver.memory.fit(policy, meminfo)

  def test_memory_fit_rejects_missing_corrupt_and_exhausted_headroom(self):
    with tempfile.TemporaryDirectory() as temp:
      policy, meminfo, _ = self.memory_fixture(Path(temp))
      for text in ('MemFree: 900000 kB\n', 'MemAvailable: -1 kB\n', 'MemAvailable: 999 MB\n',
                   'MemAvailable: 100 kB\n', 'MemAvailable: 999 kB\nMemAvailable: 888 kB\n',
                   f'MemAvailable: {1 << 64} kB\n'):
        with self.subTest(text=text):
          meminfo.write_text(text)
          with self.assertRaises(ValueError):
            driver.memory.fit(policy, meminfo)

  def test_actual_source_driver_passes_fitted_pinned_configuration_to_the_first_seed(self):
    class SeedBoundary(Exception):
      pass
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      policy, meminfo, _ = self.memory_fixture(root)
      source = root/'source'
      source.mkdir()
      def git(*args):
        subprocess.run(['/usr/bin/git', '-c', 'user.name=Owned memory fixture', '-c', 'user.email=ci@example.invalid',
            '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', '-C', source, *args], check=True,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
      git('init', '-q')
      (source/'README').write_text('source stays unchanged')
      git('add', '.')
      git('commit', '-qm', 'owned memory fixture')
      stores = {name: str(root/name) for name in ('sources', 'objects', 'generations', 'params')}
      (root/'generations').mkdir()
      (root/'params/d').mkdir(parents=True)
      (root/'params/d/IsOnroad').write_bytes(b'0')
      selection = root/'selection.json'
      selection.write_bytes(b'{}\n')
      configuration = dict(schema=1, stores=stores, cache=str(root/'cache'),
          system_sdk_receipt=str(root/'unused-sdk.json'), system_sdk_sha256='a'*64,
          expected_source_growth_bytes=1, expected_compile_growth_bytes=1, remaining_object_growth_bytes=1,
          resources=policy, runtime_inputs=dict(path=str(selection), sha256=seed.digest(selection)))
      config_path = root/'bootstrap.json'
      original = seed.json_bytes(configuration)
      config_path.write_bytes(original)
      pin = seed.digest(config_path)
      fit = driver.memory.fit
      for fault in (False, True):
        config_path.write_bytes(original)
        def seed_boundary(prepared, plan, current, checkout, **kwargs):
          first = driver.read(current)
          self.assertEqual(first['resources']['maximum_memory_bytes'], 2885681152)
          self.assertEqual(first['resources']['minimum_available_memory_bytes'], 134217728)
          self.assertEqual(config_path.read_bytes(), original)
          admitted = current.parent/'admitted-config.json'
          self.assertEqual(driver.read(admitted)['resources'], first['resources'])
          self.assertEqual(admitted.stat().st_mode & 0o777, 0o444)
          if fault:
            config_path.write_bytes(b'{}\n')
            return {}  # Original pin must be checked before this report is used.
          raise SeedBoundary('first seed received bounded configuration')
        with mock.patch.object(seed, 'verify_native'), mock.patch.object(os, 'geteuid', return_value=1000), \
             mock.patch.object(driver.memory, 'fit', side_effect=lambda p: fit(p, meminfo)), \
             mock.patch.object(seed, 'current_tools', side_effect=seed_boundary) as bootstrap, \
             mock.patch.object(driver.OwnedCommands, 'run') as command:
          exception = ValueError if fault else SeedBoundary
          message = 'pin or byte bound' if fault else 'first seed received bounded'
          with self.assertRaisesRegex(exception, message):
            driver.build_runtime(dict(root=temp), dict(protocols=dict(request_owner=2)), config_path, pin, source)
          bootstrap.assert_called_once()
          command.assert_not_called()

  def test_actual_build_cpu_scope_opens_available_cores_and_restores_on_error(self):
    # Only this child changes its own affinity; other tests and runner jobs keep
    # their existing masks. Verify a compiler child inherits the opened mask.
    program = '''
import os, subprocess, sys
import source_runtime as driver
original = os.sched_getaffinity(0)
restricted = {min(original)}
os.sched_setaffinity(0, restricted)
try:
  with driver.build_cpus() as cpus:
    assert original <= set(cpus)
    child = subprocess.check_output([sys.executable, '-c', 'import os; print(sorted(os.sched_getaffinity(0)))'], text=True)
    import ast
    assert ast.literal_eval(child) == cpus
    raise ValueError('owned compile failure')
except ValueError as error:
  assert str(error) == 'owned compile failure'
assert os.sched_getaffinity(0) == restricted
print('actual_build_affinity_inherited_and_restored=true')
'''
    result = subprocess.run([sys.executable, '-c', program],
        cwd=Path(driver.__file__).parent, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    self.assertEqual(result.returncode, 0, result.stderr.decode())
    self.assertIn(b'actual_build_affinity_inherited_and_restored=true', result.stdout)

  def test_unverified_offroad_state_never_starts_a_download_or_source_command(self):
    with tempfile.TemporaryDirectory() as temp:
      params = Path(temp)/'params'
      (params/'d').mkdir(parents=True)
      state = params/'d/IsOnroad'
      runner = driver.OwnedCommands(seed.owner_chain(), params)
      with mock.patch.object(driver.subprocess, 'Popen') as spawn:
        for value in (b'1', b'', b'00', b'0\n'):
          state.write_bytes(value)
          with self.assertRaisesRegex(ValueError, 'offroad'):
            runner.run(['/usr/bin/true'], env=seed.ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        state.unlink()
        with self.assertRaises(OSError):
          runner.run(['/usr/bin/true'], env=seed.ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        spawn.assert_not_called()

  def test_agnos_build_renews_power_lease_only_with_live_offroad_owner(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      agnos, watchdog = root/'AGNOS', root/'power_watchdog'
      agnos.touch()
      params = root/'params'
      (params/'d').mkdir(parents=True)
      state = params/'d/IsOnroad'
      state.write_bytes(b'0')
      with mock.patch.object(driver, 'AGNOS', agnos), mock.patch.object(driver, 'POWER_WATCHDOG', watchdog), \
           mock.patch.object(seed, 'verify_owners') as owners, mock.patch.object(driver.time, 'monotonic') as clock:
        runner = driver.OwnedCommands('owned-build', params)
        clock.return_value = 3601.0
        runner.observe()
        self.assertEqual(float(watchdog.read_text()), 3601.0)
        clock.return_value = 3620.0
        runner.observe()
        self.assertEqual(float(watchdog.read_text()), 3601.0)
        clock.return_value = 3631.0
        runner.observe()
        self.assertEqual(float(watchdog.read_text()), 3631.0)
        # Neither a stale/onroad state nor owner death keeps the device awake.
        clock.return_value = 7202.0
        state.write_bytes(b'1')
        with self.assertRaisesRegex(ValueError, 'offroad'):
          runner.observe()
        self.assertEqual(float(watchdog.read_text()), 3631.0)
        state.write_bytes(b'0')
        owners.side_effect = ValueError('owner exited')
        with self.assertRaisesRegex(ValueError, 'owner exited'):
          runner.observe()
        self.assertEqual(float(watchdog.read_text()), 3631.0)
        owners.side_effect = None
        self.assertIsNone(driver.OwnedCommands('copy-without-offroad-check').watchdog)
        agnos.unlink()
        self.assertIsNone(driver.OwnedCommands('host-build', params).watchdog)

  def test_power_lease_publication_replaces_links_without_writing_their_target(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      params = root/'params'
      (params/'d').mkdir(parents=True)
      (params/'d/IsOnroad').write_bytes(b'0')
      agnos, watchdog, target = root/'AGNOS', root/'power_watchdog', root/'untouched'
      agnos.touch()
      target.write_bytes(b'preserved')
      watchdog.symlink_to(target)
      with mock.patch.object(driver, 'AGNOS', agnos), mock.patch.object(driver, 'POWER_WATCHDOG', watchdog):
        driver.OwnedCommands(seed.owner_chain(), params).observe()
      self.assertFalse(watchdog.is_symlink())
      self.assertGreater(float(watchdog.read_text()), 0)
      self.assertEqual(target.read_bytes(), b'preserved')
      self.assertFalse(list(root.glob('.native-power-*')))

  def test_retained_launch_configuration_is_immutable_and_interrupted_publication_preserves_selection(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      generations = root/'generations'
      generations.mkdir()
      active = generations/'active.json'
      active.write_bytes(b'previous verified selection')
      original = root/'input.json'
      original.write_bytes(b'{"schema":1}\n')
      checksum = seed.digest(original)
      retained = driver.preserve_configuration(generations, original, checksum)
      self.assertEqual(retained.read_bytes(), original.read_bytes())
      self.assertEqual(retained.stat().st_mode & 0o777, 0o444)
      with mock.patch.object(seed, 'publish') as publish:
        self.assertEqual(driver.preserve_configuration(generations, original, checksum), retained)
        publish.assert_not_called()
      retained.chmod(0o644)
      with self.assertRaisesRegex(ValueError, 'mutable'):
        driver.preserve_configuration(generations, original, checksum)
      retained.chmod(0o444)
      original.write_bytes(b'{"schema":2}\n')
      different = seed.digest(original)
      with mock.patch.object(seed, 'publish', side_effect=OSError('owned interrupted publication')):
        with self.assertRaises(OSError):
          driver.preserve_configuration(generations, original, different)
      self.assertFalse((generations/'configurations'/(different+'.json')).exists())
      self.assertEqual(active.read_bytes(), b'previous verified selection')
      self.assertEqual(retained.read_bytes(), b'{"schema":1}\n')

  def test_tool_bundle_content_mode_inventory_and_requested_source_are_checked(self):
    with tempfile.TemporaryDirectory() as temp:
      cache = Path(temp)/'cache'
      tools = cache/'tools'/('a'*64)
      tools.mkdir(parents=True)
      source = dict(commit='b'*40, tree='c'*40)
      outputs = {}
      for name in ('openpilot-runtime-inputs', 'openpilot-runtime-native', 'openpilot-native-build-scope'):
        path = tools/name
        path.write_bytes(b'owned protocol fixture, never executed: '+name.encode())
        path.chmod(0o555)
        outputs[name] = dict(bytes=path.stat().st_size, sha256=seed.digest(path), mode=0o555)
      (tools/'manifest.json').write_text('{}')
      (tools/'manifest.json').chmod(0o444)
      report = dict(schema=1, tools=str(tools), requested_source=source,
          build=dict(source=dict(source=source), binaries=outputs), runtime_activated=False,
          runtime_started=False, runtime_release_downloaded=False)
      self.assertEqual(driver.tools_from_report(report, source, dict(cache=str(cache)))[0], tools)
      wrong = copy.deepcopy(report)
      wrong['requested_source']['commit'] = 'd'*40
      with self.assertRaises(ValueError):
        driver.tools_from_report(wrong, source, dict(cache=str(cache)))
      path = tools/'openpilot-runtime-native'
      body = path.read_bytes()
      for fault in ('mode', 'bytes', 'inventory'):
        with self.subTest(fault=fault):
          if fault == 'mode':
            path.chmod(0o755)
          elif fault == 'bytes':
            path.chmod(0o755)
            path.write_bytes(body[:-1]+bytes([body[-1] ^ 1]))
            path.chmod(0o555)
          else:
            (tools/'extra').write_text('not from the verified build')
          with self.assertRaises(ValueError):
            driver.tools_from_report(report, source, dict(cache=str(cache)))
          path.chmod(0o755)
          path.write_bytes(body)
          path.chmod(0o555)
          (tools/'extra').unlink(missing_ok=True)

  def test_seed_phase_cannot_silently_change_the_requested_clean_source(self):
    with tempfile.TemporaryDirectory() as temp:
      source = Path(temp)/'source'
      source.mkdir()
      def git(*args):
        subprocess.run(['/usr/bin/git', '-c', 'user.name=Owned source driver fixture',
            '-c', 'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false',
            '-c', 'core.hooksPath=/dev/null', '-C', source, *args], check=True,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
      git('init', '-q')
      (source/'README').write_text('original request')
      git('add', '.')
      git('commit', '-qm', 'original')
      original = driver.source_identity(source)
      (source/'README').write_text('later clean commit')
      git('add', '.')
      git('commit', '-qm', 'later')
      with mock.patch.object(seed, 'verify_native'), mock.patch.object(os, 'geteuid', return_value=1000), \
          mock.patch.object(driver.OwnedCommands, 'run') as command:
        with self.assertRaisesRegex(ValueError, 'requested source changed'):
          driver.build_runtime(dict(root=temp), dict(protocols=dict(request_owner=2)),
              Path(temp)/'unused-config', 'a'*64, source, expected_source=original)
        command.assert_not_called()

  def test_original_owner_death_stops_only_the_dedicated_command_and_bounds_capture(self):
    self.assertNotEqual(os.geteuid(), 0, 'registered native validation must retain non-root identity')
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      normal = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
      owner = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
      ready = root/'ready'
      failed = []
      def stop_owner():
        until = time.monotonic()+10
        while not ready.exists() and time.monotonic() < until:
          time.sleep(0.02)
        if not ready.exists():
          failed.append('owned command never initialized')
        owner.kill()
        owner.wait()
      killer = threading.Thread(target=stop_owner)
      try:
        runner = driver.OwnedCommands(seed.owner_chain(seed.process_identity(owner.pid)))
        killer.start()
        with self.assertRaises((ValueError, OSError)):
          # The isolated production command cannot inherit the /opt oracle's loader.
          runner.run(['/usr/bin/python3', '-I', '-B', '-S', '-c',
              'import os,pathlib,sys,time; p=pathlib.Path(sys.argv[1]); t=p.with_suffix(".pending"); t.write_text(str(os.getpid())); t.replace(p); time.sleep(60)', str(ready)],
              env=seed.ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        killer.join(timeout=12)
        self.assertFalse(killer.is_alive())
        self.assertFalse(failed)
        self.assertIsNone(normal.poll())
        pid = int(ready.read_text())
        with self.assertRaises((ValueError, OSError)):
          seed.process_identity(pid)
        runner = driver.OwnedCommands(seed.owner_chain())
        with self.assertRaisesRegex(ValueError, '4 MiB'):
          runner.run(['/usr/bin/python3', '-I', '-B', '-S', '-c', 'import sys; sys.stdout.buffer.write(b"x"*(5*1024**2))'],
              env=seed.ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        self.assertIsNone(normal.poll())
      finally:
        if killer.is_alive():
          killer.join(timeout=12)
        for child in (owner, normal):
          if child.poll() is None:
            child.kill()
          child.wait()

  def test_driver_sigkill_also_stops_the_non_root_download_command(self):
    self.assertNotEqual(os.geteuid(), 0)
    with tempfile.TemporaryDirectory() as temp:
      ready = Path(temp)/'ready'
      native = str(Path(driver.__file__).resolve().parent)
      body = ('import os,pathlib,sys,time; p=pathlib.Path(sys.argv[1]); '
              't=p.with_suffix(".pending"); t.write_text(str(os.getpid())); t.replace(p); time.sleep(60)')
      code = ('import sys,subprocess; sys.path.insert(0,sys.argv[1]); '
              'import source_runtime as d; import bootstrap_seed as s; '
              'd.OwnedCommands(s.owner_chain()).run(["/usr/bin/python3","-I","-B","-S","-c",sys.argv[3],sys.argv[2]],'
              'env=s.ENV,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True)')
      parent = subprocess.Popen([sys.executable, '-c', code, native, str(ready), body])
      identity = None
      try:
        until = time.monotonic()+10
        while not ready.exists() and parent.poll() is None and time.monotonic() < until:
          time.sleep(0.02)
        self.assertTrue(ready.exists(), 'dedicated child did not initialize')
        pid = int(ready.read_text())
        identity = seed.process_identity(pid)
        parent.kill()
        parent.wait()
        until = time.monotonic()+5
        while time.monotonic() < until:
          try:
            if seed.process_identity(pid) != identity:
              break
          except (ValueError, OSError):
            break
          time.sleep(0.02)
        else:
          self.fail('non-root command survived its driver SIGKILL')
      finally:
        if parent.poll() is None:
          parent.kill()
        parent.wait()
        if identity is not None:
          try:
            if seed.process_identity(pid) == identity:
              os.kill(pid, signal.SIGKILL)
          except (ValueError, OSError):
            pass


if __name__ == '__main__':
  unittest.main()

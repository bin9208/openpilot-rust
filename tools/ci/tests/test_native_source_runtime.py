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

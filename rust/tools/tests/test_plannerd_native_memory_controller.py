"""ASan wrapper의 실제 main을 실행한다. solver 기능 검증은 별도 Actions gate다."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import shlex
import sys
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / 'check_plannerd_native_memory.py'
CONTROLLER = r'''
import json, os, pathlib, sys
args = sys.argv[1:]
assert args[0] == 'memory'
pathlib.Path(os.environ['MEMORY_TEST_CALLS']).write_text(json.dumps(args))
mode = os.environ['MEMORY_TEST_MODE']
if mode == 'failure':
  sys.exit(42)
if mode == 'empty':
  sys.exit(0)
output = pathlib.Path(args[args.index('--output') + 1])
output.mkdir(parents=True)
names = ('abi', 'owner') if mode == 'incomplete' else ('abi', 'owner', 'fault')
(output / 'commands.json').write_text(json.dumps([{'name': name, 'returncode': 0} for name in names]))
'''


class NativeMemoryControllerTests(unittest.TestCase):
  def setUp(self):
    self.temp = tempfile.TemporaryDirectory(prefix='planner memory-')
    self.addCleanup(self.temp.cleanup)
    self.root = Path(self.temp.name)
    self.bin = self.root / 'bin'
    (self.bin / 'examples').mkdir(parents=True)
    for name in ('solver', 'owner', 'runtime'):
      (self.bin / 'examples' / f'plannerd_{name}_trace').write_bytes(name.encode())
    self.artifact = self.root / 'artifact'
    self.artifact.mkdir()
    (self.artifact / 'provenance.json').write_text(json.dumps({'sanitizers': True}))
    (self.artifact / 'manifest.json').write_bytes(b'original manifest\n')
    self.asan = self.root / 'libasan.so'
    self.asan.write_bytes(b'original ASan library')
    self.output = self.root / 'output'
    implementation = self.root / 'controller.py'
    implementation.write_text(CONTROLLER)
    self.controller = self.root / 'controller'
    self.controller.write_text('#!/bin/sh\nexec "$MEMORY_TEST_PYTHON" -u "$MEMORY_TEST_CONTROLLER" "$@"\n')
    self.controller.chmod(0o755)
    self.calls = self.root / 'calls.json'
    environment = patch.dict(os.environ, MEMORY_TEST_PYTHON=sys.executable,
                             MEMORY_TEST_CONTROLLER=str(implementation), MEMORY_TEST_CALLS=str(self.calls),
                             MEMORY_TEST_MODE='success')
    environment.start()
    self.addCleanup(environment.stop)
    self.namespace = runpy.run_path(str(SOURCE))
    self.main = self.namespace['main']
    source_root = patch.dict(self.main.__globals__, ROOT=self.root)
    source_root.start()
    self.addCleanup(source_root.stop)

  def argv(self):
    return [str(SOURCE), '--bin-dir', str(self.bin), '--artifact', str(self.artifact),
            '--source-native', str(self.root / 'source'), '--binding', str(self.root / 'params.so'),
            '--asan-library', str(self.asan), '--output', str(self.output),
            '--controller', str(self.controller), '--python', sys.executable, '--timeout-seconds', '2700',
            '--growth-mib', 'abi=128', '--growth-mib', 'owner=1024', '--growth-mib', 'fault=1024']

  def invoke(self, mode):
    os.environ['MEMORY_TEST_MODE'] = mode
    stdout = io.StringIO()
    with patch.object(sys, 'argv', self.argv()), contextlib.redirect_stdout(stdout):
      self.main()
    return json.loads(stdout.getvalue())

  def test_main_keeps_sanitizer_wrappers_hashes_and_delegates_exact_three_native_checks(self):
    result = self.invoke('success')
    self.assertEqual(result['status'], 'PASS')
    self.assertEqual(result['checks'], ['abi', 'owner', 'fault'])
    self.assertEqual(result['asan_sha256'], hashlib.sha256(self.asan.read_bytes()).hexdigest())
    self.assertEqual(result['artifact_sha256'], hashlib.sha256((self.artifact / 'manifest.json').read_bytes()).hexdigest())
    for name in ('solver', 'owner', 'runtime'):
      binary = (self.bin / 'examples' / f'plannerd_{name}_trace').resolve()
      self.assertEqual(result['binary_sha256'][str(binary)], hashlib.sha256(binary.read_bytes()).hexdigest())
      wrapper = self.output / 'wrappers/examples' / binary.name
      command = wrapper.read_text().removeprefix('#!/bin/sh\nexec ').removesuffix(' "$@"\n')
      self.assertEqual(shlex.split(command), ['env', f'LD_PRELOAD={self.asan}',
                      'ASAN_OPTIONS=detect_leaks=1:abort_on_error=1:strict_string_checks=1',
                      'UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1', str(binary)])
    args = json.loads(self.calls.read_text())
    self.assertEqual(args[0], 'memory')
    self.assertEqual(args[args.index('--root') + 1], str(self.root))
    self.assertEqual(args[args.index('--python') + 1], sys.executable)
    self.assertEqual(args[args.index('--bin-dir') + 1], str(self.output / 'wrappers'))
    self.assertEqual(args[args.index('--output') + 1], str(self.output / 'comparison'))
    self.assertEqual(args[args.index('--timeout-seconds') + 1], '2700')
    self.assertEqual([args[index + 1] for index, value in enumerate(args) if value == '--growth-mib'],
                     ['abi=128', 'owner=1024', 'fault=1024'])

  def test_native_failure_42_is_propagated_without_a_pass_receipt(self):
    with self.assertRaises(SystemExit) as error:
      self.invoke('failure')
    self.assertEqual(error.exception.code, 42)
    self.assertFalse((self.output / 'result.json').exists())

  def test_success_without_native_records_is_rejected(self):
    with self.assertRaises(FileNotFoundError):
      self.invoke('empty')
    self.assertFalse((self.output / 'result.json').exists())

  def test_incomplete_native_subset_is_rejected(self):
    with self.assertRaises(AssertionError):
      self.invoke('incomplete')
    self.assertFalse((self.output / 'result.json').exists())

  def test_unsanitized_artifact_is_rejected_before_the_controller(self):
    (self.artifact / 'provenance.json').write_text(json.dumps({'sanitizers': False}))
    with self.assertRaises(AssertionError):
      self.invoke('success')
    self.assertFalse(self.calls.exists())


if __name__ == '__main__':
  unittest.main()

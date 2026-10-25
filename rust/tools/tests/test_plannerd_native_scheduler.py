"""실제 run의 stdio adapter를 AST로 로드한다. C++/모델 import 없이 Actions에서 실행한다."""
import ast
import hashlib
import math
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / 'plannerd_ipc.py'
ORACLE_SHA256 = 'a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1'
PEER_SOURCE = r'''
import os, signal, sys, time
assert sys.argv[1:] == ['schedule']
mode = os.environ['PLANNER_TEST_MODE']
if mode == 'ignore-term':
  signal.signal(signal.SIGTERM, signal.SIG_IGN)
if mode == 'eof':
  sys.exit(0)
if mode == 'exit42':
  sys.exit(42)
for line in sys.stdin:
  assert len(line.split()) == 3
  with open(os.environ['PLANNER_TEST_REQUESTS'], 'a') as log:
    log.write(line)
  if mode == 'timeout':
    time.sleep(30)
  reply = {'malformed': 'invalid', 'nonfinite': 'nan', 'long': 'x' * 300,
           'multiple': '0.0\n1.0'}.get(mode, '0.0')
  # multiple은 한 write의 두 행이다.
  sys.stdout.write(reply + '\n')
  sys.stdout.flush()
if mode == 'close42':
  sys.exit(42)
if mode == 'ignore-term':
  time.sleep(30)
'''


def load_scheduler():
  tree = ast.parse(SOURCE.read_bytes())
  namespace = dict(__name__='planner_scheduler_source', math=math, os=os, select=select,
                   subprocess=subprocess, sys=sys, time=time)
  nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef))
           and node.name in ('next_model_target', 'ModelScheduler')]
  if len(nodes) != 2:
    raise AssertionError('actual IPC scheduler and independent oracle are required')
  exec(compile(ast.Module(nodes, type_ignores=[]), str(SOURCE), 'exec'), namespace)
  return namespace


class NativeSchedulerTests(unittest.TestCase):
  def setUp(self):
    self.temp = tempfile.TemporaryDirectory(prefix='planner-scheduler-')
    self.addCleanup(self.temp.cleanup)
    self.root = Path(self.temp.name)
    peer = self.root / 'peer.py'
    peer.write_text(PEER_SOURCE)
    self.binary = self.root / 'native-scheduler'
    self.binary.write_text('#!/bin/sh\nexec "$PLANNER_TEST_PYTHON" -u "$PLANNER_TEST_PEER" "$@"\n')
    self.binary.chmod(0o755)
    self.requests = self.root / 'requests.tsv'
    self.environment = patch.dict(os.environ, PLANNER_NATIVE_SCHEDULER=str(self.binary),
                                  PLANNER_TEST_PYTHON=sys.executable, PLANNER_TEST_PEER=str(peer),
                                  PLANNER_TEST_REQUESTS=str(self.requests), PLANNER_TEST_MODE='normal')
    self.environment.start()
    self.addCleanup(self.environment.stop)
    self.namespace = load_scheduler()
    self.Scheduler = self.namespace['ModelScheduler']

  def peer(self):
    return self.Scheduler(self.root / 'scheduler.stderr.log')

  def forbidden_oracle(self):
    return patch.dict(self.namespace, next_model_target=lambda *_: self.fail('native error fell back to the oracle'))

  def test_independent_oracle_bytes_are_unchanged(self):
    raw = SOURCE.read_bytes()
    node = next(node for node in ast.parse(raw).body if isinstance(node, ast.FunctionDef) and node.name == 'next_model_target')
    function = b''.join(raw.splitlines(keepends=True)[node.lineno - 1:node.end_lineno])
    self.assertEqual(hashlib.sha256(function).hexdigest(), ORACLE_SHA256)

  def test_actual_run_loop_uses_one_scheduler_context_and_one_request_per_frame(self):
    run = next(node for node in ast.parse(SOURCE.read_bytes()).body if isinstance(node, ast.FunctionDef) and node.name == 'run')
    contexts = [item for node in ast.walk(run) if isinstance(node, ast.With) for item in node.items
                if isinstance(item.context_expr, ast.Call) and isinstance(item.context_expr.func, ast.Name)
                and item.context_expr.func.id == 'ModelScheduler']
    self.assertEqual(len(contexts), 1)
    self.assertEqual(contexts[0].optional_vars.id, 'scheduler')
    loop = next(node for node in ast.walk(run) if isinstance(node, ast.For) and isinstance(node.target, ast.Name) and node.target.id == 'index')
    requests = [node for node in ast.walk(loop) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and isinstance(node.func.value, ast.Name) and node.func.value.id == 'scheduler' and node.func.attr == 'target']
    self.assertEqual(len(requests), 1)
    self.assertFalse(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'next_model_target'
                         for node in ast.walk(run)))

  def test_absent_environment_preserves_oracle_signed_zero_and_nan_without_spawning(self):
    os.environ.pop('PLANNER_NATIVE_SCHEDULER')
    oracle = self.namespace['next_model_target']
    with patch.object(subprocess, 'Popen') as spawn:
      with self.peer() as peer:
        for case in ((-0.0, -0.05, -0.05), (float('nan'), 100.0, 100.08)):
          self.assertEqual(struct.pack('>d', peer.target(*case)), struct.pack('>d', oracle(*case)))
      spawn.assert_not_called()
    self.assertFalse((self.root / 'scheduler.stderr.log').exists())

  def test_persistent_process_serves_forty_requests_and_is_reaped_on_eof(self):
    real_spawn = subprocess.Popen
    with self.forbidden_oracle(), patch.object(subprocess, 'Popen', wraps=real_spawn) as spawn:
      with self.peer() as peer:
        for index in range(40):
          self.assertEqual(peer.target(100.05 + index * 0.05, 100.0, 100.003), 0.0)
      self.assertEqual(spawn.call_count, 1)
      self.assertEqual(peer.process.returncode, 0)
      self.assertTrue(peer.process.stdin.closed and peer.process.stdout.closed)
    self.assertEqual(len(self.requests.read_text().splitlines()), 40)

  def test_empty_setting_is_an_error_instead_of_oracle_mode(self):
    os.environ['PLANNER_NATIVE_SCHEDULER'] = ''
    with self.forbidden_oracle(), patch.object(subprocess, 'Popen') as spawn:
      with self.assertRaises(RuntimeError):
        with self.peer():
          self.fail('empty native setting was accepted')
      spawn.assert_not_called()

  def test_spawn_failure_does_not_fall_back(self):
    os.environ['PLANNER_NATIVE_SCHEDULER'] = str(self.root / 'missing')
    with self.forbidden_oracle(), self.assertRaises(FileNotFoundError):
      with self.peer():
        self.fail('missing native executable was accepted')

  def test_eof_exit_and_invalid_response_fail_closed_and_reap(self):
    for mode in ('eof', 'exit42', 'malformed', 'nonfinite', 'long', 'multiple'):
      with self.subTest(mode=mode):
        os.environ['PLANNER_TEST_MODE'] = mode
        peer = self.peer()
        with self.forbidden_oracle(), self.assertRaises((RuntimeError, OSError, ValueError)):
          with peer:
            peer.target(100.05, 100.0, 100.003)
        self.assertTrue(peer.failed)
        self.assertIsNotNone(peer.process.returncode)

  def test_response_timeout_fails_closed_and_reaps(self):
    os.environ['PLANNER_TEST_MODE'] = 'timeout'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaises(TimeoutError):
      with peer:
        peer.target(100.05, 100.0, 100.003)
    self.assertIsNotNone(peer.process.returncode)

  def test_caught_eof_does_not_allow_retry_or_successful_context_exit(self):
    os.environ['PLANNER_TEST_MODE'] = 'eof'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaises(RuntimeError):
      with peer:
        with self.assertRaises((RuntimeError, OSError)):
          peer.target(100.05, 100.0, 100.003)
        with self.assertRaises(RuntimeError):
          peer.target(100.05, 100.0, 100.003)
    self.assertEqual(peer.process.returncode, 0)

  def test_abnormal_close_status_is_not_silently_accepted(self):
    os.environ['PLANNER_TEST_MODE'] = 'close42'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaisesRegex(RuntimeError, '42'):
      with peer:
        self.assertEqual(peer.target(100.05, 100.0, 100.003), 0.0)
    self.assertEqual(peer.process.returncode, 42)

  def test_normal_close_escalates_term_kill_wait_and_reports_failure(self):
    os.environ['PLANNER_TEST_MODE'] = 'ignore-term'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaises(RuntimeError):
      with peer:
        self.assertEqual(peer.target(100.05, 100.0, 100.003), 0.0)
    self.assertEqual(peer.process.returncode, -signal.SIGKILL)

  def test_exception_cleanup_reaps_stubborn_child_and_preserves_parent_error(self):
    os.environ['PLANNER_TEST_MODE'] = 'ignore-term'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaisesRegex(ValueError, 'parent run failed'):
      with peer:
        self.assertEqual(peer.target(100.05, 100.0, 100.003), 0.0)
        raise ValueError('parent run failed')
    self.assertEqual(peer.process.returncode, -signal.SIGKILL)

  @unittest.skipUnless(os.environ.get('PLANNER_NATIVE_SCHEDULER_TEST_BIN'), 'Actions native binary registration required')
  def test_real_native_binary_receives_all_160_calls_with_matching_targets(self):
    os.environ['PLANNER_NATIVE_SCHEDULER'] = os.environ['PLANNER_NATIVE_SCHEDULER_TEST_BIN']
    evidence = self.root / 'native-scheduler.tsv'
    os.environ['PLANNER_NATIVE_SCHEDULER_EVIDENCE'] = str(evidence)
    oracle = self.namespace['next_model_target']
    real_spawn = subprocess.Popen
    with patch.object(subprocess, 'Popen', wraps=real_spawn) as spawn:
      for run in range(4):
        with self.Scheduler(self.root / f'scheduler-{run}.stderr.log') as peer:
          for index in range(40):
            boundaries = ((-0.0, -0.05, -0.05), (0.0, -0.05, -0.05),
                          (-0.0, -0.1, -0.05), (0.0, -0.1, -0.05))
            case = boundaries[index] if index < len(boundaries) else (
              100.05 + index * 0.05, 100.0 + index * 0.05, 100.003 + index * 0.05,
            )
            self.assertEqual(struct.pack('>d', peer.target(*case)), struct.pack('>d', oracle(*case)))
        self.assertEqual(peer.process.returncode, 0)
      self.assertEqual(spawn.call_count, 4)
    rows = evidence.read_text().splitlines()
    self.assertEqual(len(rows), 160)
    for row in rows:
      nominal, sent, done, target = map(float, row.split())
      self.assertEqual(struct.pack('>d', target), struct.pack('>d', oracle(nominal, sent, done)))


if __name__ == '__main__':
  unittest.main()

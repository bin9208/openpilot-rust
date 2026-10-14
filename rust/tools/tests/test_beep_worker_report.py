"""Owned thread-reporting boundaries around unchanged original Beepd methods."""
from contextlib import redirect_stderr
import io
from pathlib import Path
import subprocess
import sys
import threading
from types import SimpleNamespace
import unittest  # noqa: TID251 - focused stdlib fixture regression
from unittest.mock import patch  # noqa: TID251 - focused stdlib fixture regression

import beep_source


class WorkerReporting(unittest.TestCase):
  def original(self, beep):
    workers = []
    thread = threading.Thread
    def owned_thread(*args, **kwargs):
      worker = thread(*args, **kwargs)
      workers.append(worker)
      return worker
    scope = {'threading': SimpleNamespace(Thread=owned_thread), 'time': SimpleNamespace(sleep=lambda _: None)}
    path = Path(__file__).resolve().parents[3] / 'openpilot/selfdrive/controls/beep.py'
    cls = beep_source.definition(path, 'Beepd', scope)
    instance = cls.__new__(cls)
    instance._beep = beep
    return instance, workers

  def test_isolated_collection_requires_no_native_binding(self):
    root = Path(__file__).resolve().parents[3]
    script = """
import importlib.abc
import pathlib
import sys
import unittest
root = pathlib.Path(sys.argv[1])
sys.path[:0] = [str(root / 'rust/tools/tests'), str(root / 'rust/tools'), str(root)]
class NoNative(importlib.abc.MetaPathFinder):
  def find_spec(self, fullname, path, target=None):
    if fullname == 'msgq' or fullname.startswith('msgq.') or fullname in ['openpilot.cereal', 'openpilot.common.params_pyx']:
      raise ModuleNotFoundError('controlled native bindings unavailable: ' + fullname)
sys.meta_path.insert(0, NoNative())
import beep_source
import test_beep_worker_report
suite = unittest.defaultTestLoader.loadTestsFromModule(test_beep_worker_report)
assert suite.countTestCases() == 6
assert 'openpilot.cereal' not in sys.modules and 'openpilot.common.params_pyx' not in sys.modules
"""
    subprocess.run([sys.executable, '-I', '-c', script, str(root)], check=True, timeout=10)

  def test_blocked_exception_presentation_keeps_already_failed_worker_alive(self):
    entered = threading.Event()
    release = threading.Event()
    failed = threading.Event()
    def failure(_):
      failed.set()
      raise FileNotFoundError('owned command failure')
    def blocked_report(args):
      entered.set()
      release.wait(2)
    instance, workers = self.original(failure)
    try:
      with patch.object(threading, 'excepthook', blocked_report):
        instance.dispatch_beep(instance.engage)
        self.assertTrue(failed.wait(1) and entered.wait(1))
        workers[0].join(timeout=0.05)
        self.assertTrue(workers[0].is_alive())
        release.set()
        workers[0].join(timeout=1)
        self.assertFalse(workers[0].is_alive())
    finally:
      release.set()
      for worker in workers:
        worker.join(timeout=2)

  def test_concise_report_bypasses_blocked_formatter_and_keeps_failure_visible(self):
    def failure(_):
      raise FileNotFoundError('owned command failure')
    instance, workers = self.original(failure)
    output = io.StringIO()
    with patch.object(threading, 'excepthook', side_effect=AssertionError('default formatter used')) as original:
      with redirect_stderr(output), beep_source.policy_worker_reports():
        instance.dispatch_beep(instance.engage)
        workers[0].join(timeout=1)
        self.assertFalse(workers[0].is_alive())
      original.assert_not_called()
    self.assertIn('FileNotFoundError: owned command failure', output.getvalue())
    self.assertIn(workers[0].name, output.getvalue())
    self.assertIn(str(workers[0].ident), output.getvalue())

  def test_hook_does_not_mark_a_running_worker_finished(self):
    entered = threading.Event()
    release = threading.Event()
    def waiting(_):
      entered.set()
      release.wait(2)
    instance, workers = self.original(waiting)
    try:
      with beep_source.policy_worker_reports():
        instance.dispatch_beep(instance.engage)
        self.assertTrue(entered.wait(1))
        workers[0].join(timeout=0.05)
        self.assertTrue(workers[0].is_alive())
        release.set()
        workers[0].join(timeout=1)
        self.assertFalse(workers[0].is_alive())
    finally:
      release.set()
      for worker in workers:
        worker.join(timeout=2)

  def test_system_exit_remains_silent(self):
    output = io.StringIO()
    with redirect_stderr(output), beep_source.policy_worker_reports():
      threading.excepthook(SimpleNamespace(exc_type=SystemExit, exc_value=SystemExit(7), thread=None))
    self.assertEqual(output.getvalue(), '')
    class ChildExit(SystemExit):
      pass
    with redirect_stderr(output), beep_source.policy_worker_reports():
      threading.excepthook(SimpleNamespace(exc_type=ChildExit, exc_value=ChildExit(7), thread=threading.current_thread()))
    self.assertIn('ChildExit: 7', output.getvalue())

  def test_hook_is_restored_after_main_fixture_failure(self):
    original = threading.excepthook
    with self.assertRaisesRegex(RuntimeError, 'owned main failure'), beep_source.policy_worker_reports():
      raise RuntimeError('owned main failure')
    self.assertIs(threading.excepthook, original)


if __name__ == '__main__':
  unittest.main()

import importlib.util
from pathlib import Path
import unittest  # noqa: TID251 - explicit stdlib-only fixture gate
import os
import tempfile
import sys
from unittest.mock import Mock, patch  # noqa: TID251 - explicit stdlib-only fixture gate

ROOT = Path(__file__).resolve().parents[3]
PEER = ROOT / 'rust/tools/web_upload_eintr_peer.py'

def load(path):
  spec = importlib.util.spec_from_file_location('owned_eintr_peer', path)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module

class ChildDiscovery(unittest.TestCase):
  path = PEER
  def setUp(self):
    self.peer = load(self.path)
    self.tracer = Mock(pid=987654)
    self.tracer.poll.return_value = None
  def test_each_attempt_uses_one_children_snapshot(self):
    children = Mock()
    children.exists.return_value = True
    children.read_text.side_effect = ['123', '']
    with patch.object(self.peer, 'Path', return_value=children):
      self.assertEqual(self.peer.child_pid(self.tracer), 123)
    self.assertEqual(children.read_text.call_count, 1)

  def test_disappearing_proc_entry_is_retried_with_one_read_per_attempt(self):
    children = Mock()
    children.read_text.side_effect = [FileNotFoundError(), '  123  ']
    with patch.object(self.peer, 'Path', return_value=children):
      self.assertEqual(self.peer.child_pid(self.tracer), 123)
    self.assertEqual(children.read_text.call_count, 2)

  def test_empty_ambiguous_and_nonpositive_children_are_never_selected(self):
    for value in ['', '123 456', '0', '-1', str(self.tracer.pid), '١٢٣']:
      children = Mock()
      children.read_text.side_effect = [value, '123']
      with self.subTest(value=value), patch.object(self.peer, 'Path', return_value=children):
        self.assertEqual(self.peer.child_pid(self.tracer), 123)
      self.assertEqual(children.read_text.call_count, 2)

  def test_exited_tracer_is_a_real_failure_not_an_empty_discovery(self):
    self.tracer.poll.return_value = 17
    self.tracer.returncode = 17
    with self.assertRaisesRegex(RuntimeError, 'exited.*17'):
      self.peer.child_pid(self.tracer)

  def test_early_real_child_failure_keeps_status_stderr_and_reaps_tracer(self):
    seen = []
    original = self.peer.subprocess.Popen
    def spawn(*args, **kwargs):
      process = original(*args, **kwargs)
      seen.append(process)
      return process
    discover = self.peer.child_pid
    def after_exit(process):
      process.wait(timeout=3)
      return discover(process)
    with (
      tempfile.TemporaryDirectory() as temp,
      patch.object(self.peer.subprocess, 'Popen', side_effect=spawn),
      patch.object(self.peer, 'child_pid', side_effect=after_exit),
    ):
      with self.assertRaisesRegex(RuntimeError, 'tracer exit 17; stderr:.*owned early failure'):
        self.peer.launch([sys.executable, '-c', "import sys;sys.stderr.write('owned early failure');sys.exit(17)"], Path(temp)/'run', {}, dict(os.environ))
    self.assertEqual(len(seen), 1)
    self.assertEqual(seen[0].poll(), 17)
    self.assertTrue(seen[0].stdout.closed)

  def test_discovery_failure_kills_only_owned_tracer_and_tracee(self):
    seen = []
    discover = self.peer.child_pid
    def fail_after_discovery(process):
      pid = discover(process)
      seen.append((process, pid))
      os.kill(pid, self.peer.signal.SIGSTOP)
      self.peer.until(lambda: Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] in ('T', 't'))
      raise RuntimeError('owned discovery failure')
    with tempfile.TemporaryDirectory() as temp, patch.object(self.peer, 'child_pid', side_effect=fail_after_discovery):
      with self.assertRaisesRegex(RuntimeError, 'owned discovery failure'):
        self.peer.launch([sys.executable, '-c', 'import time;time.sleep(30)'], Path(temp)/'run', {}, dict(os.environ))
    process, pid = seen[0]
    self.assertIsNotNone(process.poll())
    def stopped():
      try:
        return Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[0] == 'Z'
      except FileNotFoundError:
        return True
    self.peer.until(stopped)

  def test_compare_closes_peer_when_launch_fails(self):
    module_path = self.path.with_name('web_upload_eintr_compare.py')
    with patch.dict(sys.modules, web_upload_eintr_peer=self.peer):
      compare = load(module_path)
    recipient = self.peer.Peer()
    try:
      with patch.object(compare, 'Peer', return_value=recipient), patch.object(compare, 'launch', side_effect=RuntimeError('owned launch failed')):
        with self.assertRaisesRegex(RuntimeError, 'owned launch failed'):
          compare.run('source', Path('/not-used'), Path('/not-used'), False)
      self.assertTrue(recipient.closed)
      self.assertFalse(recipient.thread.is_alive())
    finally:
      recipient.close()

  def test_compare_closes_peer_even_if_reaping_fails(self):
    module_path = self.path.with_name('web_upload_eintr_compare.py')
    with patch.dict(sys.modules, web_upload_eintr_peer=self.peer):
      compare = load(module_path)
    recipient = self.peer.Peer()
    try:
      with (
        patch.object(compare, 'Peer', return_value=recipient),
        patch.object(compare, 'launch', return_value=(Mock(), 123, Path('/not-used'), [], {})),
        patch.object(compare, 'until', side_effect=RuntimeError('owned run failed')),
        patch.object(compare, 'stop_child', side_effect=RuntimeError('owned cleanup failed')),
      ):
        with self.assertRaisesRegex(RuntimeError, 'owned cleanup failed'):
          compare.run('source', Path('/not-used'), Path('/not-used'), False)
      self.assertTrue(recipient.closed)
      self.assertFalse(recipient.thread.is_alive())
    finally:
      recipient.close()

if __name__ == '__main__':
  unittest.main()

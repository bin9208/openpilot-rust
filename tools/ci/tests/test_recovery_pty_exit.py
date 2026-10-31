"""Actual child exits after terminal EOF, before its status is observable."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import unittest

SOURCE = Path(__file__).resolve().parents[3]/'openpilot/selfdrive/carrot/recovery/server.py'
spec = importlib.util.spec_from_file_location('recovery_pty_exit_source', SOURCE)
server = importlib.util.module_from_spec(spec)
spec.loader.exec_module(server)


class Client:
  def __init__(self):
    self.events = []
    self.closed = False

  def send_json(self, value):
    self.events.append(value)
    return True

  def close(self):
    self.closed = True


class PtyExitTests(unittest.TestCase):
  def test_eof_before_exit_preserves_actual_zero_and_nonzero_status(self):
    for code in (0, 7):
      with self.subTest(code=code):
        # stdout closes first, exactly the ordering that makes a single poll
        # after terminal EIO racy. The child exits only after the EOF.
        child = subprocess.Popen([sys.executable, '-c',
            f'import os,time; os.close(1); time.sleep(0.3); os._exit({code})'],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            start_new_session=True)
        terminal, client = server.RecoveryPty(), Client()
        terminal.proc = child
        terminal.master_fd = os.dup(child.stdout.fileno())
        terminal.clients.add(client)
        try:
          terminal._read_loop()
          self.assertEqual(client.events, [dict(type='pty_exit', session=terminal.session, exit_code=code)])
          self.assertEqual(child.returncode, code)
          self.assertTrue(client.closed)
          self.assertIsNone(terminal.proc)
          self.assertEqual(terminal.master_fd, -1)
        finally:
          terminal.terminate()
          child.stdout.close()
          if child.poll() is None: child.kill()
          child.wait(timeout=3)


if __name__ == '__main__':
  unittest.main()

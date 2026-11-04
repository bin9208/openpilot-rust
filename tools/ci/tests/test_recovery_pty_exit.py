"""Actual child exits after terminal EOF, before its status is observable."""
import importlib.util
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import AsyncMock
from types import SimpleNamespace
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[3]/'rust/tools'))
from recovery_exit_status import install_exit_status_barrier
from carrot_recovery_terminal_control import Ws

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
      with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
        # stdout closes first, exactly the ordering that makes a single poll
        # after terminal EIO racy. The child exits only after the EOF.
        child = subprocess.Popen([sys.executable, '-c',
            f'import os,time; os.close(1); time.sleep(0.3); os._exit({code})'],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            start_new_session=True)
        # A fresh original source class is used per case; only the listener's
        # explicit exit-status observation boundary is installed.
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        class OwnedClient(Client): pass
        module.WsConn = OwnedClient
        install_exit_status_barrier(module, Path(temporary))
        terminal, client = module.RecoveryPty(), OwnedClient()
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


class TerminalOutputTests(unittest.IsolatedAsyncioTestCase):
  @staticmethod
  def output(data):
    return 1, json.dumps(dict(type='pty_output', b64=base64.b64encode(data).decode())).encode()

  @staticmethod
  def running():
    return 1, b'{"type":"command_running","id":"owned-command"}'

  async def test_output_before_or_after_running_remains_in_wire_order(self):
    for early in (True, False):
      with self.subTest(output_before_running=early):
        chunks = [self.output(b'APPRO'), self.output(b'VED=263\r\n')]
        frames = chunks + [self.running()] if early else [self.running()] + chunks
        peer = Ws(None, None, [])
        peer.frame = AsyncMock(side_effect=frames)
        self.assertEqual((await peer.event('command_running'))['id'], 'owned-command')
        self.assertEqual(await peer.output_until(b'APPROVED=263\r\n'), b'APPROVED=263\r\n')
        self.assertEqual(peer.frame.await_count, 3)
        self.assertFalse(peer.pending_output)

  async def test_missing_marker_still_fails_with_original_receive_deadline(self):
    peer = Ws(None, None, [])
    peer.frame = AsyncMock(side_effect=[self.output(b'different output'), self.running(), TimeoutError()])
    await peer.event('command_running')
    with self.assertRaises(TimeoutError):
      await peer.output_until(b'APPROVED=263\r\n')
    peer.frame.assert_awaited_with(8)

  async def test_buffer_is_owned_by_one_peer_and_close_still_fails(self):
    first, second = Ws(None, None, []), Ws(None, None, [])
    first.frame = AsyncMock(side_effect=[self.output(b'APPROVED=263\r\n'), self.running()])
    second.frame = AsyncMock(side_effect=[self.running(), (8, b'')])
    await first.event('command_running')
    await second.event('command_running')
    with self.assertRaisesRegex(AssertionError, 'closed before event'):
      await second.output_until(b'APPROVED=263\r\n')
    self.assertEqual(await first.output_until(b'APPROVED=263\r\n'), b'APPROVED=263\r\n')


if __name__ == '__main__':
  unittest.main()

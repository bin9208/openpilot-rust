# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: python rust/tools/test_athena_camera_children.py
import ast
import errno
import os
from pathlib import Path
import subprocess
import sys
from types import ModuleType
import unittest
from unittest.mock import patch


def discovery() -> ModuleType:
  path = Path(__file__).with_name('check_athena_camera_lifecycle.py')
  tree = ast.parse(path.read_text(), filename=str(path))
  functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ('children', 'camera_children')]
  module = ModuleType('owned_camera_discovery')
  module.Path = Path
  exec(compile(ast.Module(body=functions, type_ignores=[]), str(path), 'exec'), module.__dict__)
  return module


class CameraChildrenTests(unittest.TestCase):
  def test_disappeared_child_keeps_following_live_camera(self) -> None:
    module = discovery()
    camera = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
    disappeared = subprocess.Popen([sys.executable, '-c', 'pass'])
    try:
      disappeared.wait(timeout=5)
      gone = Path(f'/proc/{disappeared.pid}/exe')
      resolve = Path.resolve

      def racing(path: Path, *args, **kwargs) -> Path:
        if path == gone:
          raise FileNotFoundError(errno.ENOENT, os.strerror(errno.ENOENT), str(path))
        return resolve(path, *args, **kwargs)

      module.children = lambda _: [disappeared.pid, camera.pid]
      with patch.object(Path, 'resolve', racing):
        self.assertEqual(module.camera_children(os.getpid(), Path(sys.executable)), [camera.pid])
      self.assertIsNone(camera.poll())
    finally:
      camera.terminate()
      camera.wait(timeout=5)
      if disappeared.poll() is None:
        disappeared.terminate()
        disappeared.wait(timeout=5)

  def test_permission_error_propagates(self) -> None:
    module = discovery()
    module.children = lambda _: [73000001]
    error = PermissionError(errno.EACCES, 'owned permission failure')
    with patch.object(Path, 'resolve', side_effect=error):
      with self.assertRaises(PermissionError) as caught:
        module.camera_children(os.getpid(), Path(sys.executable))
    self.assertIs(caught.exception, error)

  def test_other_os_error_propagates(self) -> None:
    module = discovery()
    module.children = lambda _: [73000001]
    error = OSError(errno.EIO, 'owned I/O failure')
    with patch.object(Path, 'resolve', side_effect=error):
      with self.assertRaises(OSError) as caught:
        module.camera_children(os.getpid(), Path(sys.executable))
    self.assertIs(caught.exception, error)

  def test_missing_expected_executable_propagates(self) -> None:
    module = discovery()
    module.children = lambda _: [73000001]
    error = FileNotFoundError(errno.ENOENT, 'owned expected-executable failure')
    with patch.object(Path, 'resolve', side_effect=[Path('/owned/camera'), error]):
      with self.assertRaises(FileNotFoundError) as caught:
        module.camera_children(os.getpid(), Path('/owned/expected'))
    self.assertIs(caught.exception, error)

  def test_parent_children_read_failure_propagates(self) -> None:
    module = discovery()
    error = FileNotFoundError(errno.ENOENT, 'owned parent disappeared')
    with patch.object(Path, 'read_text', side_effect=error):
      with self.assertRaises(FileNotFoundError) as caught:
        module.camera_children(os.getpid(), Path(sys.executable))
    self.assertIs(caught.exception, error)

  def test_actual_owned_child_identity_and_mismatch(self) -> None:
    module = discovery()
    child = subprocess.Popen(['/usr/bin/sleep', '30'])
    try:
      self.assertIn(child.pid, module.children(os.getpid()))
      self.assertEqual(module.camera_children(os.getpid(), Path('/usr/bin/sleep')), [child.pid])
      self.assertNotIn(child.pid, module.camera_children(os.getpid(), Path(sys.executable)))
      self.assertIsNone(child.poll())
    finally:
      child.terminate()
      child.wait(timeout=5)


if __name__ == '__main__':
  unittest.main(verbosity=2)

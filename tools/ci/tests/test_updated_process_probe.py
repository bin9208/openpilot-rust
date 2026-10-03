import unittest
from unittest.mock import patch

from rust.tools.check_updated_process import alive


class UpdatedProcessProbeTests(unittest.TestCase):
    def test_exited_process_during_open_or_read_is_not_alive(self):
        for error in (FileNotFoundError(2, 'gone'), ProcessLookupError(3, 'gone during read')):
            with self.subTest(error=type(error)), patch('pathlib.Path.read_text', side_effect=error):
                self.assertFalse(alive(123))

    def test_zombie_is_stopped_and_running_worker_is_alive(self):
        for state, expected in [('Z', False), ('S', True)]:
            with patch('pathlib.Path.read_text', return_value=f'123 (worker) {state} 1 2 3'):
                self.assertEqual(alive(123), expected)

    def test_permission_failure_is_not_an_exit(self):
        with patch('pathlib.Path.read_text', side_effect=PermissionError(13, 'denied')):
            with self.assertRaises(PermissionError):
                alive(123)


if __name__ == '__main__':
    unittest.main()

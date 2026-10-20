"""Observe real process disappearance without hiding live or inaccessible PIDs."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


def fixture():
    path = ROOT / 'rust/tools/tests/test_web_upload_eintr_peer.py'
    spec = importlib.util.spec_from_file_location('eintr_cleanup_fixture', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class EintrCleanupTests(unittest.TestCase):
    def test_reaped_or_zombie_process_is_stopped(self):
        module = fixture()
        for outcome in (FileNotFoundError(), ProcessLookupError(), '42 (owned child) Z 0'):
            with self.subTest(outcome=outcome), patch.object(module.Path, 'read_text') as read:
                if isinstance(outcome, Exception):
                    read.side_effect = outcome
                else:
                    read.return_value = outcome
                self.assertTrue(module.process_stopped(42))
                read.assert_called_once()

    def test_live_process_and_real_io_errors_are_not_success(self):
        module = fixture()
        for state in ('S', 'T', 't', 'R'):
            with self.subTest(state=state), patch.object(module.Path, 'read_text', return_value=f'42 (owned child) {state} 0'):
                self.assertFalse(module.process_stopped(42))
        with patch.object(module.Path, 'read_text', side_effect=PermissionError('denied')):
            with self.assertRaises(PermissionError):
                module.process_stopped(42)


if __name__ == '__main__':
    unittest.main()

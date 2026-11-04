"""Require immutable-file rename/link semantics in the real CI user space."""
import os
from pathlib import Path
import stat
import tempfile
import unittest


class ReadonlyPublicationTests(unittest.TestCase):
    def test_rename_of_readonly_data_and_executable_keeps_exact_mode_and_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for mode in (0o444, 0o555):
                with self.subTest(mode=oct(mode)):
                    source, destination = root/'pending', root/'published'
                    source.write_bytes(b'owned immutable publication fixture\n')
                    source.chmod(mode)
                    os.rename(source, destination)
                    actual = stat.S_IMODE(destination.stat().st_mode)
                    print(f'readonly rename uid={os.geteuid()} expected={mode:o} actual={actual:o}', flush=True)
                    self.assertEqual(actual, mode)
                    with destination.open('rb') as stream:
                        self.assertEqual(stat.S_IMODE(os.fstat(stream.fileno()).st_mode), mode)
                        self.assertEqual(stream.read(), b'owned immutable publication fixture\n')
                    destination.unlink()

    def test_removing_one_readonly_hardlink_preserves_the_retained_object(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for mode in (0o444, 0o555):
                with self.subTest(mode=oct(mode)):
                    source, retained = root/'candidate', root/'retained'
                    source.write_bytes(b'owned retained object\n')
                    source.chmod(mode)
                    os.link(source, retained)
                    self.assertEqual(source.stat().st_ino, retained.stat().st_ino)
                    source.unlink()
                    actual = stat.S_IMODE(retained.stat().st_mode)
                    print(f'readonly unlink uid={os.geteuid()} expected={mode:o} actual={actual:o}', flush=True)
                    self.assertEqual(actual, mode)
                    self.assertEqual(retained.read_bytes(), b'owned retained object\n')
                    retained.unlink()


if __name__ == '__main__':
    unittest.main()

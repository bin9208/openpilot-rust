from contextlib import contextmanager, redirect_stdout
import io
import json
from pathlib import Path
from types import SimpleNamespace
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'rust/tools'))
import check_cweb_address as checker


@contextmanager
def invocation(free, actual=None):
    with tempfile.TemporaryDirectory() as temporary:
        output = Path(temporary) / 'address'
        addresses = ['127.0.0.1', None, None, None]
        with mock.patch.object(checker.argparse.ArgumentParser, 'parse_args', return_value=
                SimpleNamespace(output=output, binary=Path(temporary) / 'cweb-address')), \
                mock.patch.object(checker.shutil, 'disk_usage', return_value=SimpleNamespace(free=free)), \
                mock.patch.object(checker.subprocess, 'run') as native, \
                mock.patch.object(checker.subprocess, 'check_output', return_value=
                    json.dumps(addresses if actual is None else actual).encode()) as address, \
                mock.patch.object(checker, 'module', return_value={
                    '_iface_ipv4': lambda name: '127.0.0.1' if name == 'lo' else None}) as source, \
                redirect_stdout(io.StringIO()):
            yield output, native, address, source


class CwebAddressAdmissionTests(unittest.TestCase):
    def test_below_retired_floor_keeps_all_native_and_source_checks(self):
        with invocation(24 * 1024**3) as (output, native, address, source):
            checker.main()
            self.assertEqual(native.call_count, 3)
            for call in native.call_args_list[:2]:
                self.assertIn('-fsanitize=address,undefined', call.args[0])
                self.assertTrue(call.kwargs['check'])
            self.assertTrue(native.call_args_list[2].kwargs['check'])
            self.assertEqual(address.call_args.args[0][1:],
                ['lo', 'cweb-nonexistent-interface', '', '한글없는인터페이스'])
            source.assert_called_once()
            result = json.loads((output / 'result.json').read_text())
            self.assertTrue(result['source_native_equal'])
            self.assertTrue(result['ioctl_ownership_asan_ubsan'])

    def test_exact_original_operation_growth_is_admitted(self):
        with invocation(1024**3) as (_, native, _, _):
            checker.main()
            self.assertEqual(native.call_count, 3)

    def test_shortage_stops_before_native_work_or_oracle(self):
        with invocation(1024**3 - 1) as (output, native, address, source):
            with self.assertRaises(AssertionError):
                checker.main()
            native.assert_not_called()
            address.assert_not_called()
            source.assert_not_called()
            self.assertFalse((output / 'result.json').exists())

    def test_source_mismatch_is_still_a_failure(self):
        with invocation(24 * 1024**3, ['incorrect'] * 4) as (output, native, _, _):
            with self.assertRaises(AssertionError):
                checker.main()
            self.assertEqual(native.call_count, 3)
            self.assertFalse((output / 'result.json').exists())


if __name__ == '__main__':
    unittest.main()

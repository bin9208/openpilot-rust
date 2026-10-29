"""Actual DBC output pins under a small synthetic capacity, not altered data."""
from contextlib import redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT/'opendbc_repo'))
SPEC = importlib.util.spec_from_file_location('owned_dbc_ci', ROOT/'rust/tools/radarcan_ci_builder.py')
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)


class RadarcanCiBuilderTests(unittest.TestCase):
    def test_original_dbc_bytes_match_every_pinned_output_without_the_retired_floor(self):
        original = builder.load_original(builder.ORIGINAL.read_bytes())
        contract = json.loads((ROOT/'rust/native-dbc-source-contract.json').read_text())
        expected = {Path(name).name: pin for name, pin in contract['outputs'].items() if name.endswith('.dbc')}
        python_path = os.pathsep.join(filter(None, (str(ROOT/'opendbc_repo'), os.environ.get('PYTHONPATH'))))
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)/'dbc'
            # A unit capacity fixture, never a report of actual available space.
            # generate_all starts the unchanged generator in a fresh Python.
            # That child cannot inherit this test module's sys.path insertion.
            with patch.dict(os.environ, PYTHONPATH=python_path), \
                    patch.object(original.shutil, 'disk_usage', return_value=SimpleNamespace(free=1024**3)):
                with redirect_stdout(io.StringIO()):
                    original.prepare(target)
            actual = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in target.glob('*.dbc')}
            self.assertEqual(actual, expected)
            record = json.loads((target/'radarcan-assets.json').read_text())
            self.assertEqual(record['free_before'], 1024**3)
            self.assertLess(record['growth_bound'], record['free_before'])

    def test_insufficient_operation_growth_fails_before_generation_or_destination_creation(self):
        original = builder.load_original(builder.ORIGINAL.read_bytes())
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)/'must-not-exist'
            with patch.object(original.shutil, 'disk_usage', return_value=SimpleNamespace(free=0)):
                with patch.object(original, 'generate_all', side_effect=AssertionError('generation ran before admission')):
                    with self.assertRaises(OSError):
                        original.prepare(target)
            self.assertFalse(target.exists())

    def test_mutated_generator_is_rejected_before_execution(self):
        with self.assertRaisesRegex(ValueError, 'exact frozen generator bytes'):
            builder.load_original(builder.ORIGINAL.read_bytes()+b'\nraise AssertionError("not frozen")\n')


if __name__ == '__main__':
    unittest.main()

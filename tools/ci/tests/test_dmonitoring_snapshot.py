"""The replay wake-up must not race the oracle's complete input snapshot."""
import ast
from pathlib import Path
import unittest
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]


class MonitoringSnapshotTests(unittest.TestCase):
    def test_polled_model_wakes_only_after_other_inputs_are_queued(self):
        source = ast.parse((ROOT / 'rust/tools/check_dmonitoring_daemon.py').read_text())
        function = next(node for node in source.body if isinstance(node, ast.FunctionDef)
                        and node.name == 'publish_inputs')
        scope = {}
        exec(compile(ast.Module(body=[function], type_ignores=[]), '<snapshot>', 'exec'), scope)
        for camera_present in (False, True):
            with self.subTest(camera_present=camera_present):
                names = ['carState', 'selfdriveState', 'modelV2', 'radarState', 'liveCalibration']
                if camera_present:
                    names.append('driverStateV2')
                encoded = {name: name.encode() for name in names}
                queued = {}
                snapshots = []

                def send(name, payload):
                    queued[name] = payload
                    if name == 'modelV2':
                        snapshots.append(dict(queued))

                publishers = {name: SimpleNamespace(send=lambda payload, name=name: send(name, payload))
                              for name in names}
                scope['publish_inputs'](publishers, encoded)
                self.assertEqual(snapshots, [encoded])


if __name__ == '__main__':
    unittest.main()

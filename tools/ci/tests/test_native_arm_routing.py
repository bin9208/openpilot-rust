"""Native ARM admission and the production comma identity remain explicit."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT/'tools/ci'/f'{name}.py')
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


policy = module('runner_policy')
transport = module('probe_native_source_transport')
ARM = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted',
           RUNNER_OS='Linux', RUNNER_ARCH='ARM64', AGNOS_NATIVE_HOSTED_ARM='1')


class NativeArmRoutingTests(unittest.TestCase):
    def test_only_the_explicit_linux_arm_host_joins_private_agnos_admission(self):
        with patch.dict(os.environ, ARM, clear=True):
            policy.require_agnos_environment()
        for key, value in (('GITHUB_ACTIONS', 'false'), ('RUNNER_ARCH', 'X64'),
                           ('RUNNER_OS', 'Windows'), ('AGNOS_NATIVE_HOSTED_ARM', '0')):
            with self.subTest(key=key), patch.dict(os.environ, {**ARM, key: value}, clear=True):
                with self.assertRaises(ValueError):
                    policy.require_agnos_environment()

    def test_registered_x64_admission_remains_available_for_historical_probes(self):
        for name in ('bin-actions', 'bin-actions2', 'bin-actions3'):
            with patch.dict(os.environ, dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted',
                                            RUNNER_NAME=name), clear=True):
                policy.require_agnos_environment()

    def test_root_and_every_identity_are_preserved_while_comma_maps_to_the_runner(self):
        with patch.dict(os.environ, ARM, clear=True):
            for runner in (1000, 1001, 2000):
                with self.subTest(runner=runner):
                    mapping = {}
                    for line in transport.namespace_mapping(runner).decode().splitlines():
                        inside, outside, count = map(int, line.split())
                        for offset in range(count):
                            self.assertNotIn(inside+offset, mapping)
                            mapping[inside+offset] = outside+offset
                    self.assertEqual(set(mapping), set(range(65536)))
                    self.assertEqual(set(mapping.values()), set(range(65536)))
                    self.assertEqual(mapping[0], 0)
                    self.assertEqual(mapping[1000], runner)
                    self.assertEqual(mapping[runner], 1000)
            for invalid in (0, -1, 65536):
                with self.assertRaises(AssertionError):
                    transport.namespace_mapping(invalid)

    def test_the_factory_caller_stays_comma_with_no_change_to_legacy_ids(self):
        with patch.dict(os.environ, ARM, clear=True):
            self.assertEqual(transport.factory_ids(1001, 1001), (1000, 1000))
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(transport.factory_ids(1234, 1235), (1234, 1235))
            self.assertEqual(transport.namespace_mapping(1234), b'0 0 65536\n')


if __name__ == '__main__':
    unittest.main()

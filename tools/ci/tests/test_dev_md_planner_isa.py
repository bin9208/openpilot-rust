"""Regressions for dev-md's explicit host-only planner SDK adapter."""
import hashlib
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import yaml

ROOT = Path(__file__).resolve().parents[3]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT/'rust/tools'/filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


SDK = module('dev_md_host_sdk', 'plannerd_host_sdk.py')
with patch.dict(sys.modules, plannerd_host_sdk=SDK):
    ADAPTER = module('dev_md_ci_adapter', 'plannerd_ci_builder.py')


class PlannerIsaTests(unittest.TestCase):
    def test_dev_md_source_fixture_builder_bytes_remain_fixed(self):
        original = ROOT/'rust/tools/build_plannerd_acados.py'
        self.assertEqual(hashlib.sha256(original.read_bytes()).hexdigest(), ADAPTER.BUILDER_SHA256)

    def test_changed_builder_is_rejected_before_its_code_executes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            builder = root/'rust/tools/build_plannerd_acados.py'
            builder.parent.mkdir(parents=True)
            builder.write_text('raise AssertionError("changed source must not execute")\n')
            context = dict(GITHUB_ACTIONS='true', RUNNER_NAME='bin-actions', RUNNER_ENVIRONMENT='self-hosted')
            with patch.object(ADAPTER, 'ROOT', root), patch.dict(os.environ, context), \
                    patch.object(sys, 'argv', ['adapter', '--', str(builder)]):
                with self.assertRaisesRegex(ValueError, 'fixed bytes'):
                    ADAPTER.main()

    def test_compatible_host_keeps_the_original_wheel(self):
        with patch.object(SDK.platform, 'machine', return_value='x86_64'), \
                patch.object(SDK, 'flags', return_value={'avx2', 'fma'}):
            self.assertIsNone(SDK.prepare())

    def test_incompatible_host_cannot_build_outside_registered_actions(self):
        with patch.object(SDK.platform, 'machine', return_value='x86_64'), \
                patch.object(SDK, 'flags', return_value={'avx'}), patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, 'Actions-only'):
                SDK.prepare()

    def test_qpoases_original_and_rendered_notice_both_remain_pinned(self):
        import json
        pins = json.loads((ROOT/'rust/crates/plannerd/licenses/sources.json').read_text())
        row = next(row for row in pins['licenses'] if row['name'] == 'qpoases')
        notice = ROOT/'rust/crates/plannerd/licenses/qpoases.txt'
        with tempfile.TemporaryDirectory() as temporary:
            original = Path(temporary)/'LICENSE.txt'
            original.write_bytes(notice.read_bytes().replace(b'^L', b'\x0c'))
            record = SDK.verify_license(row, original, notice)
            self.assertEqual(record['visible_form_feeds'], 9)
            original.write_bytes(original.read_bytes()+b'changed')
            with self.assertRaisesRegex(ValueError, 'license mismatch'):
                SDK.verify_license(row, original, notice)

    def test_original_planner_argv_stays_after_explicit_adapter(self):
        workflow = yaml.safe_load((ROOT/'.github/workflows/ci.yml').read_text())
        steps = workflow['jobs']['planner-runtime']['steps']
        lines = [line.strip() for step in steps for line in step.get('run', '').splitlines()
                 if line.strip().startswith('python rust/tools/plannerd_ci_builder.py -- ')]
        normalized = [line.replace('python rust/tools/plannerd_ci_builder.py -- ', 'python ', 1) for line in lines]
        expected = [
            'python rust/tools/build_plannerd_acados.py --source-fixture rust/upstream/planner-source.json --architecture "$(uname -m)" --cc "$planner_cc" --python "$(command -v python)" --acados "$PLANNER_GENERATOR" --wheel "$RUNNER_TEMP"/planner-wheel/*.whl --output "$RUNNER_TEMP/planner-artifact"',
            'python rust/tools/build_plannerd_source.py --artifact "$RUNNER_TEMP/planner-artifact" --python "$(command -v python)" --acados "$PLANNER_GENERATOR" --generator-wheel "$RUNNER_TEMP"/planner-wheel/*.whl --cc "$planner_cc" --output "$RUNNER_TEMP/planner-source"',
            'python rust/tools/build_plannerd_acados.py --source-fixture rust/upstream/planner-source.json --architecture x86_64 --cc gcc --sanitize --python "$(command -v python)" --acados "$PLANNER_GENERATOR" --wheel "$RUNNER_TEMP"/planner-wheel/*.whl --output "$RUNNER_TEMP/planner-sanitized"',
        ]
        self.assertEqual(normalized, expected)
        self.assertEqual(workflow['jobs']['planner-runtime']['strategy']['matrix']['runner'],
                         ['local-x64', 'local-arm64-qemu'])


if __name__ == '__main__':
    unittest.main()

"""Prevent inherited publishers from acting in the independent Rust repository."""
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]

class RustIsolationTests(unittest.TestCase):
    def test_inherited_side_effects_are_source_repository_only(self):
        for file, job in [('sync.yml', 'sync'), ('naver-upstream-sync.yml', 'sync'), ('wiki-settings-publish.yaml', 'synchronize'), ('carrot-route-vault-publish.yaml', 'publish')]:
            with self.subTest(file=file):
                data = yaml.load((ROOT / '.github/workflows' / file).read_text(), Loader=yaml.BaseLoader)
                self.assertIn("github.repository == 'bin9208/openpilot'", data['jobs'][job].get('if', ''))

    def test_rust_checks_cover_push_and_protected_prs(self):
        data = yaml.load((ROOT / '.github/workflows/rust.yml').read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(data['on']['push']['branches'], ['**'])
        self.assertEqual(set(data['on']['pull_request']['branches']), {'dev', 'main'})
        for event in ['push', 'pull_request']:
            self.assertNotIn('paths', data['on'][event])
            self.assertNotIn('paths-ignore', data['on'][event])
        gate = data['jobs']['fast']
        self.assertEqual(gate['if'], '${{ always() }}')
        self.assertEqual(set(gate['needs']), {'model-memory', 'model-pipelines', 'logger-runtime'})
        validation = next(step for step in gate['steps'] if step.get('name') == 'Require model memory and pipeline validation')
        self.assertEqual(validation['env'], {'MEMORY': '${{ needs.model-memory.result }}', 'PIPELINES': '${{ needs.model-pipelines.result }}',
                                            'LOGGER': '${{ needs.logger-runtime.result }}'})
        for name in ('MEMORY', 'PIPELINES', 'LOGGER'):
            self.assertIn(f'test "${name}" = success', validation['run'])
        for job in data['jobs'].values():
            self.assertNotIn('continue-on-error', job)

if __name__ == '__main__':
    unittest.main()

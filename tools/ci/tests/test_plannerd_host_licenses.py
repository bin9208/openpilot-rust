"""Reject raw source/notice mutation while preserving the frozen ARM notices."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT/'rust/tools'))
from plannerd_host_sdk import QPOASES_LICENSE_SHA256, verify_license


class PinnedHostLicenseTests(unittest.TestCase):
    def setUp(self):
        self.root = ROOT/'rust/crates/plannerd/licenses'
        self.rows = json.loads((self.root/'sources.json').read_text())['licenses']
        self.qpoases = next(row for row in self.rows if row['name'] == 'qpoases')

    def test_actual_source_bytes_and_all_frozen_notices_are_independently_pinned(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)/'source-license'
            for row in self.rows:
                with self.subTest(name=row['name']):
                    notice = self.root/(row['name']+'.txt')
                    source.write_bytes(notice.read_bytes().replace(b'^L', b'\x0c')
                                       if row['name'] == 'qpoases' else notice.read_bytes())
                    result = verify_license(row, source, notice)
                    self.assertEqual(result['bundled_notice_sha256'], row['sha256'])
                    if row['name'] == 'qpoases':
                        self.assertEqual(result['source_sha256'], QPOASES_LICENSE_SHA256)
                        self.assertEqual(result['visible_form_feeds'], 9)

    def test_already_rendered_notice_is_not_accepted_as_the_upstream_source(self):
        notice = self.root/'qpoases.txt'
        with self.assertRaisesRegex(ValueError, 'source expected='):
            verify_license(self.qpoases, notice, notice)

    def test_license_text_mutation_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)/'source-license'
            source.write_bytes((self.root/'qpoases.txt').read_bytes().replace(b'^L', b'\x0c')+b'changed')
            with self.assertRaisesRegex(ValueError, 'source expected='):
                verify_license(self.qpoases, source, self.root/'qpoases.txt')

    def test_bundled_notice_mutation_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            source, notice = (Path(temporary)/name for name in ('source', 'notice'))
            original = (self.root/'qpoases.txt').read_bytes()
            source.write_bytes(original.replace(b'^L', b'\x0c'))
            notice.write_bytes(original+b'changed')
            with self.assertRaisesRegex(ValueError, 'notice expected='):
                verify_license(self.qpoases, source, notice)


if __name__ == '__main__':
    unittest.main()

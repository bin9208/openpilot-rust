"""Cold GM setup retains the measured loopback corpus and exact CAN gate."""
import ast
import copy
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


ROOT = Path(__file__).resolve().parents[3]


def functions(**scope):
    path = ROOT / 'rust/tools/check_card_runtime_pumped.py'
    tree = ast.parse(path.read_text())
    nodes = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)]
    nodes += [node for node in tree.body if isinstance(node, ast.FunctionDef)
              and node.name in ('fixture', 'comparison_output', 'compare_case')]
    exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(path), 'exec'), scope)
    return scope


class CardWarmupTests(unittest.TestCase):
    def test_only_volt_warmup_defers_loopback_and_all_measured_frames_are_retained(self):
        packets = [[{'address': 384, 'bus': bus, 'data': [tick]} for bus in (0, 2, 128)]
                   for tick in range(80)]
        retained = copy.deepcopy(packets)
        scope = functions(EXTENDED_BRANDS={'CHEVROLET_VOLT': 'gm', 'OTHER': 'other'},
                          extended_frames=lambda *_: packets, SERVICES=('carControl',),
                          wire=lambda *_args, **_kwargs: b'event')
        rows = scope['fixture']('CHEVROLET_VOLT')
        self.assertEqual(rows[0]['frames'], [frame for frame in retained[0] if frame['bus'] != 128])
        self.assertEqual([row['frames'] for row in rows[1:]], retained)
        self.assertEqual(packets, retained)
        self.assertEqual(scope['fixture']('OTHER')[0]['frames'], retained[0])
        self.assertEqual(len(rows), 81)

    def test_warmup_comparison_keeps_every_frame_byte_order_and_validity(self):
        output = functions()['comparison_output']
        source = {'warmup': {'sendcan': [{'logMonoTime': 10, 'valid': True,
                                        'sendcan': [{'address': 384, 'src': 0, 'dat': [16, 0, 15, 255]}]},
                                       {'logMonoTime': 20, 'valid': False, 'sendcan': []}]}}
        native = copy.deepcopy(source)
        native['warmup']['sendcan'][0]['logMonoTime'] = 999
        measured = {'sendcan': [{'valid': True, 'sendcan': []}]}
        expected = output(source, measured)
        self.assertEqual(expected, output(native, measured))
        self.assertEqual(source['warmup']['sendcan'][0]['logMonoTime'], 10)
        for change in ('missing', 'bytes', 'validity', 'order'):
            altered = copy.deepcopy(native)
            rows = altered['warmup']['sendcan']
            if change == 'missing':
                rows.pop()
            elif change == 'bytes':
                rows[0]['sendcan'][0]['dat'][0] = 17
            elif change == 'validity':
                rows[0]['valid'] = False
            else:
                rows.reverse()
            self.assertNotEqual(expected, output(altered, measured), change)
        self.assertIs(expected['measured'], measured)

    def test_comparable_warmup_difference_fails_the_real_case_without_retry(self):
        with patch.object(sys, 'path', [str(ROOT / 'rust/tools'), *sys.path]):
            from card_qa.runtime_admission import Attempt, run_aligned
        source = {'params': {}, 'passive': False,
                  'warmup': {'sendcan': [{'valid': True, 'sendcan': [{'address': 384}]}]}}
        native = copy.deepcopy(source)
        native['warmup']['sendcan'][0]['sendcan'] = []
        capture = Mock(side_effect=[source, native])

        def compare(left, right):
            self.assertEqual(left, right)

        scope = functions(json=json, Attempt=Attempt, run_aligned=run_aligned,
                          build_registry=lambda *_: {}, wire_identity=lambda _: None,
                          replace=lambda _, evidence: SimpleNamespace(evidence=evidence),
                          capture=capture, measured=lambda *_: {'sendcan': []},
                          input_history=lambda *_: [], compare=compare)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaises(AssertionError):
                scope['compare_case'](SimpleNamespace(evidence=root),
                                      SimpleNamespace(candidate='CHEVROLET_VOLT', enabled=True), b'[]')
            case = root / 'CHEVROLET_VOLT-True'
            events = json.loads((case / 'attempts.json').read_text())
            self.assertEqual(events[-1]['status'], 'output_failure')
            self.assertFalse((case / 'attempt-02').exists())
            self.assertEqual(capture.call_count, 2)


if __name__ == '__main__':
    unittest.main()

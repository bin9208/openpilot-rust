import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT/'rust/tools'
sys.path.insert(0, str(TOOLS))
import ci_cases
import check_card_ci as card
import check_ui_ci as ui


def original(name):
  code = subprocess.check_output(['git', 'show',
      '44fbc7a34526950063713fd8255b96b0e543f554:rust/tools/'+name+'.py'], cwd=ROOT, text=True)
  module = importlib.util.module_from_spec(importlib.util.spec_from_loader('old_'+name, loader=None))
  exec(compile(code, name, 'exec'), module.__dict__)
  return module


class OriginalCasesTests(unittest.TestCase):
  def test_card_keeps_every_original_case_and_argument(self):
    old = original('check_card_ci')
    args = mock.Mock(**{name:Path('/owned')/name for name in
        ('binaries', 'dbc', 'numerics', 'evidence', 'binding')})
    expected, actual = [], []
    with mock.patch.object(old, 'require_space'), mock.patch.object(old, 'run',
        side_effect=lambda argv, output, name: expected.append((name, argv))):
      old.reference(args)
    with mock.patch.object(card, 'run_cases', side_effect=lambda cases, *a:
        actual.extend((c['name'], c['argv']) for c in cases)):
      card.reference(args)
    self.assertEqual(actual, expected)

  def test_ui_groups_cover_each_original_case_once_with_same_arguments(self):
    old = original('check_ui_ci')
    expected, actual = [], []
    env = dict(STARTUP_UI_RAYLIB_ROOT='/owned/raylib', STARTUP_UI_RAYLIB_LIBRARY='/owned/raylib.so')
    with mock.patch.dict(os.environ, env), mock.patch.object(old, 'require_space'), \
         mock.patch.object(old, 'run', side_effect=lambda argv, output, name: expected.append((name, argv))):
      old.UiCheck(Path('/owned/output'), Path('/owned/bin'), Path('/owned/peer')).execute(':42')
    for group in ui.GROUPS:
      with mock.patch.dict(os.environ, env), mock.patch.object(ui, 'require_space'), \
           mock.patch.object(ui, 'run', side_effect=lambda argv, output, name: actual.append((name, argv))):
        check = ui.UiCheck(Path('/owned/output'), Path('/owned/bin'), Path('/owned/peer'), group)
        check.execute(':42')
        self.assertEqual(set(check.completed), set(ui.GROUPS[group]))
    self.assertEqual(sorted(actual), sorted(expected))
    self.assertEqual(len({n for n, _ in actual}), len(expected))


class SchedulerTests(unittest.TestCase):
  def test_real_children_own_temporary_and_ipc_namespaces_and_failure_is_not_pass(self):
    for fail in (False, True):
      with self.subTest(fail=fail), tempfile.TemporaryDirectory() as temporary:
        output = Path(temporary)
        code = ('import json,os,pathlib,sys; p=pathlib.Path(os.environ["TMPDIR"]); '
            '(p/"same-name").write_text(os.environ["OPENPILOT_PREFIX"]); '
            'print(json.dumps([str(p),os.environ["OPENPILOT_PREFIX"]])); sys.exit(int(sys.argv[1]))')
        cases = [dict(name=name, argv=[sys.executable, '-c', code, str(int(fail and name == 'b'))],
                      memory_bytes=ci_cases.MIB, growth_bytes=ci_cases.MIB) for name in ('a', 'b')]
        with mock.patch.object(ci_cases.os, 'sched_getaffinity', return_value={1, 3}), \
             mock.patch.object(ci_cases, 'available_memory', return_value=1024*ci_cases.MIB):
          if fail:
            with self.assertRaises(RuntimeError): ci_cases.run_cases(cases, output, ROOT)
          else: ci_cases.run_cases(cases, output, ROOT)
        summary = json.loads((output/'summary.json').read_text())
        self.assertEqual(summary['success'], not fail)
        self.assertEqual(set(summary['expected_cases']), {'a', 'b'})
        if not fail:
          results = [json.loads((output/name/'stdout').read_text()) for name in ('a', 'b')]
          self.assertNotEqual(*[r[0] for r in results])
          self.assertNotEqual(*[r[1] for r in results])

  def test_prepared_inputs_are_readonly_during_cases_and_restored_on_failure(self):
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      data = root/'input'
      data.write_text('prepared')
      mode = data.stat().st_mode
      with mock.patch.dict(os.environ, dict(CI_JOB_TEMP=str(root))):
        with self.assertRaisesRegex(RuntimeError, 'case failure'):
          with ci_cases.readonly_inputs([data]):
            self.assertEqual(data.stat().st_mode & 0o222, 0)
            result = subprocess.run([sys.executable, '-c',
                'import pathlib,sys; pathlib.Path(sys.argv[1]).write_text("changed")', str(data)],
                capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            raise RuntimeError('case failure')
      self.assertEqual(data.read_text(), 'prepared')
      self.assertEqual(data.stat().st_mode, mode)

  def test_insufficient_combined_resources_fall_back_to_one_and_never_change_cargo_limits(self):
    cases = [dict(memory_bytes=1024*ci_cases.MIB, growth_bytes=ci_cases.MIB)]*3
    before = {key:os.environ.get(key) for key in ('CARGO_BUILD_JOBS', 'RUST_TEST_THREADS')}
    with tempfile.TemporaryDirectory() as output, \
         mock.patch.object(ci_cases, 'available_memory', return_value=1536*ci_cases.MIB), \
         mock.patch.object(ci_cases.os, 'sched_getaffinity', return_value={0, 2, 4}):
      self.assertEqual(ci_cases.admission(output, cases)['workers'], 1)
    self.assertEqual(before, {key:os.environ.get(key) for key in before})


if __name__ == '__main__':
  unittest.main()

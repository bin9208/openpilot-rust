"""Device acceptance must not reuse another SHA or hide runtime deletions."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('focused_validation', ROOT / 'tools/ci/focused_validation.py')
focused = importlib.util.module_from_spec(spec)
spec.loader.exec_module(focused)


class DeviceAcceptanceTests(unittest.TestCase):
  def pr(self, sha='a' * 40):
    return dict(head=dict(sha=sha), body='\n'.join((
        'Device-Tested-SHA: ' + sha,
        'Device-Environment: C3X / AGNOS 19.8-carrot-bt1',
        'Device-Build: passed', 'Device-Boot: passed',
        'Device-Affected-Behavior: passed',
        'Device-Test-Evidence: https://github.com/bin9208/openpilot-rust/issues/598#issuecomment-123')))

  def test_current_device_evidence_accepts_and_every_incomplete_result_blocks(self):
    pr = self.pr()
    self.assertIn('사용자 실기', focused.require_device(['launch_openpilot.sh'], pr))
    for token in ('Device-Build: passed', 'Device-Boot: passed', 'Device-Affected-Behavior: passed',
                  'Device-Environment: C3X / AGNOS 19.8-carrot-bt1',
                  'Device-Test-Evidence: https://github.com/bin9208/openpilot-rust/issues/598#issuecomment-123'):
      with self.subTest(token=token), self.assertRaises(ValueError):
        focused.require_device(['launch_openpilot.sh'], dict(pr, body=pr['body'].replace(token, '')))
    with self.assertRaises(ValueError):
      focused.require_device(['tools/native/source_runtime.py'], dict(pr, head=dict(sha='b' * 40)))
    with self.assertRaises(ValueError):
      focused.require_device(['launch_openpilot.sh'], dict(pr, body=pr['body'] + '\nDevice-Boot: passed'))

  def test_documentation_exception_requires_reason_and_never_covers_runtime(self):
    pr = dict(head=dict(sha='a' * 40), body='Device-Test-Not-Needed: CI 정책과 문서만 바뀌어 기기 실행에는 영향이 없습니다.')
    self.assertIn('생략', focused.require_device(['AGENTS.md', '.github/workflows/rust.yml'], pr))
    with self.assertRaises(ValueError):
      focused.require_device(['docs/design.md', 'rust/crates/manager/src/main.rs'], pr)
    with self.assertRaises(ValueError):
      focused.require_device(['AGENTS.md'], dict(pr, body='Device-Test-Not-Needed: 없음'))
    with self.assertRaises(ValueError):
      focused.require_device([], pr)

  def test_source_helpers_select_their_short_regressions_without_workspace_tests(self):
    selected = focused.selected_tests(['tools/native/build_memory.py'])
    self.assertIn('test_native_source_runtime.py', selected)
    self.assertIn('test_native_source_entry.py', selected)
    self.assertNotIn('test_rust_isolation.py', selected)

  def test_heavy_workflows_are_opt_in_and_quick_checks_have_no_dispatch_bypass(self):
    import yaml
    for path in (ROOT / '.github/workflows').glob('*'):
      if path.suffix not in ('.yml', '.yaml'):
        continue
      data = yaml.safe_load(path.read_text())
      events = data.get('on', data.get(True, {}))
      if path.name in ('focused-validation.yml', 'user-docs.yaml'):
        self.assertNotIn('workflow_dispatch', events, path.name)
      else:
        self.assertFalse({'push', 'pull_request', 'schedule', 'workflow_run'} & events.keys(), path.name)


if __name__ == '__main__':
  unittest.main()

"""The live controller oracle must not import retired fixed checker CLIs."""
import ast
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace as NS

ROOT = Path(__file__).resolve().parents[3]


def test_controller_oracle_imports_without_retired_checker_modules():
  assert not (ROOT / 'rust/tools/check_alert_callbacks.py').exists()
  assert not (ROOT / 'rust/tools/check_selfdrive_helpers.py').exists()
  # Keep the runner's real msgq binding path ahead of the unbuilt source tree.
  # This uses the actual source oracle, not a fake messaging namespace.
  subprocess.run([sys.executable, '-P', '-c',
                  "from selfdrived_source import Fixture; assert Fixture.state_machine_class().__name__ == 'StateMachine'"],
                 check=True, capture_output=True, text=True)


def test_live_tool_imports_do_not_depend_on_removed_checker_modules():
  tools = ROOT / 'rust/tools'
  missing = []
  for path in tools.rglob('*.py'):
    for node in ast.walk(ast.parse(path.read_text())):
      if isinstance(node, ast.ImportFrom) and node.level == 0:
        names = [node.module]
      elif isinstance(node, ast.Import):
        names = [alias.name for alias in node.names]
      else:
        continue
      for name in names:
        if name and name.startswith('check_') and not (tools / (name.split('.')[0] + '.py')).is_file():
          missing.append((str(path.relative_to(ROOT)), node.lineno, name))
  assert not missing


def test_excessive_actuation_loader_uses_the_live_source_and_constants(tmp_path, monkeypatch):
  import selfdrived_helpers_source as source
  paths = [
    'openpilot/selfdrive/selfdrived/helpers.py', 'openpilot/common/realtime.py',
    'opendbc_repo/opendbc/car/__init__.py', 'opendbc_repo/opendbc/car/lateral.py',
    'opendbc_repo/opendbc/car/interfaces.py',
  ]
  for name in paths:
    target = tmp_path / name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes((ROOT / name).read_bytes())
  monkeypatch.setattr(source, 'ROOT', tmp_path)
  cls = source.source_check()
  assert cls.update.__code__.co_filename == str(tmp_path / paths[0])
  constants = cls.update.__globals__
  assert constants['MIN_EXCESSIVE_ACTUATION_COUNT'] == 25
  assert constants['MIN_LATERAL_ENGAGE_BUFFER'] == 100
  timing = tmp_path / paths[1]
  timing.write_text(timing.read_text().replace('DT_CTRL = 0.01', 'DT_CTRL = 0.02'))
  changed = source.source_check()
  assert changed.update.__globals__['MIN_EXCESSIVE_ACTUATION_COUNT'] == 12
  assert changed.update.__globals__['MIN_LATERAL_ENGAGE_BUFFER'] == 50
  machine = changed()
  sm = {'carControl': NS(longActive=True, latActive=False), 'liveParameters': NS(roll=0.0)}
  cs = NS(aEgo=11.0, vEgo=0.0, steeringPressed=False)
  pose = NS(acceleration=NS(x=11.0), angular_velocity=NS(yaw=0.0))
  assert [machine.update(sm, cs, pose) for _ in range(12)] == [None] * 12
  assert machine.update(sm, cs, pose) == 'longitudinal'


def test_event_callback_loader_keeps_actual_classes_translations_and_constants():
  from selfdrived_helpers_source import setup_source
  import json
  languages = json.loads((ROOT / 'openpilot/selfdrive/ui/translations/languages.json').read_text()).values()
  for mici in (False, True):
    for language in languages:
      source = setup_source(mici, language)
      callback = source['below_steer_speed_alert']
      assert callback.__code__.co_filename == str(ROOT / 'openpilot/selfdrive/selfdrived/events.py')
      assert source['MIN_SPEED_FILTER'] > 0
      assert source['SAMPLE_RATE'] == 16000
      assert source['SAMPLE_BUFFER'] > 0
      alert = callback(NS(minSteerSpeed=10.0), NS(), NS(), True, 0, None)
      assert isinstance(alert, source['Alert'])
      assert isinstance(alert.alert_text_1, str)
      assert isinstance(source['tr']('TAKE CONTROL IMMEDIATELY'), str)

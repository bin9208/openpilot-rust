import json
from pathlib import Path
import shlex
import tomllib

import pytest
import yaml

from check_ui_ci import vision_peer
from card_qa.ci import ROOT


def test_vision_peer_uses_current_build_receipt_with_stale_cache_present(tmp_path: Path) -> None:
  current = tmp_path / 'current output'
  stale = tmp_path / 'stale output'
  for directory in (current, stale):
    directory.mkdir()
    (directory / 'native-vision-peer').write_bytes(b'fixture peer')
  messages = tmp_path / 'build.jsonl'
  messages.write_text('\n'.join(json.dumps(row) for row in (
    {'reason': 'compiler-artifact', 'package_id': 'path+fixture#openpilot-msgq@0.1.0', 'out_dir': str(stale)},
    {'reason': 'build-script-executed', 'package_id': 'path+fixture#openpilot-msgq@0.1.0', 'out_dir': str(current)},
    {'reason': 'build-finished', 'success': True},
  )))
  assert vision_peer(messages) == current / 'native-vision-peer'


@pytest.mark.parametrize('outputs', [[], ['first', 'second']])
def test_vision_peer_rejects_absent_or_ambiguous_build_outputs(tmp_path: Path, outputs: list[str]) -> None:
  messages = tmp_path / 'build.jsonl'
  messages.write_text('\n'.join(json.dumps({'reason': 'build-script-executed',
    'package_id': 'path+fixture#openpilot-msgq@0.1.0', 'out_dir': str(tmp_path / name)}) for name in outputs))
  with pytest.raises(ValueError, match='expected one executed openpilot-msgq build output'):
    vision_peer(messages)


def test_vision_peer_rejects_missing_executable(tmp_path: Path) -> None:
  messages = tmp_path / 'build.jsonl'
  messages.write_text(json.dumps({'reason': 'build-script-executed',
    'package_id': 'path+fixture#openpilot-msgq@0.1.0', 'out_dir': str(tmp_path)}))
  with pytest.raises(FileNotFoundError):
    vision_peer(messages)


def test_arm_ui_commands_resolve_workspace_and_python_paths() -> None:
  workflow = yaml.safe_load((ROOT / '.github/workflows/rust.yml').read_text())
  job = workflow['jobs']['arm64']
  step = next(step for step in job['steps'] if 'cargo build -p openpilot-ui-application' in step.get('run', ''))
  directory = ROOT / step.get('working-directory', job.get('defaults', {}).get('run', {}).get('working-directory', '.'))
  scripts = [shlex.split(line)[1] for line in step['run'].splitlines() if line.strip().startswith('python ')]
  for script in scripts:
    assert (directory / script).is_file(), directory / script
  guards = [shlex.split(line) for line in step['run'].splitlines()
            if 'check-disk-space' in line or 'card_ci_space.py' in line]
  assert guards == [
    ['$GITHUB_WORKSPACE/rust/tools/check-disk-space', '--path', '$GITHUB_WORKSPACE', '--growth-mib', str(growth)]
    for growth in [2048, 256, 2048, 256]
  ]
  for guard in guards:
    executable = Path(guard[0].replace('$GITHUB_WORKSPACE', str(ROOT)))
    assert executable.is_absolute() and executable.is_file(), executable
  workspace = tomllib.loads((directory / 'Cargo.toml').read_text())
  assert 'crates/ui-application' in workspace['workspace']['members']


def test_ui_suite_runs_both_native_adapter_sanitizers_on_owned_display(tmp_path: Path, monkeypatch) -> None:
  import check_ui_ci as module
  old_calls, native_calls, ordered = [], [], []
  binaries, output, raylib = tmp_path / 'binaries', tmp_path / 'output', tmp_path / 'raylib'
  monkeypatch.setenv('STARTUP_UI_RAYLIB_ROOT', str(raylib))
  monkeypatch.setenv('STARTUP_UI_RAYLIB_LIBRARY', str(raylib / 'lib/libopenpilot-raylib.so'))
  def original_check(self, name, script, arguments):
    old_calls.append(name)
    ordered.append(name)
  def native_check(command, directory, name):
    native_calls.append((command, directory, name))
    ordered.append(name)
  monkeypatch.setattr(module.UiCheck, 'check', original_check)
  monkeypatch.setattr(module, 'require_space', lambda *args: None)
  monkeypatch.setattr(module, 'run', native_check)
  owner = module.UiCheck(output, binaries, tmp_path / 'peer')
  owner.execute('owned-display')
  assert old_calls == ['state', 'scheduler', 'root', 'augmented', 'camera', 'runtime', 'pages',
                       'hud-eight-modes', 'hud-traffic', 'calibration']
  assert ordered == ['state', 'scheduler', 'qr', 'root', 'augmented', 'camera', 'runtime', 'pages',
                     'hud-eight-modes', 'hud-traffic', 'calibration', 'torque', 'native-safety',
                     'native-model-adapter', 'native-training-adapter']
  assert native_calls == [
    ([str(binaries / 'check-ui-qr'), '--binary', str(binaries / 'examples/qr_trace'),
      '--output', str(output / 'qr')], output / 'commands', 'qr'),
    ([str(binaries / 'check-ui-torque-geometry'), '--binary', str(binaries / 'examples/torque_geometry'),
      '--output', str(output / 'torque')], output / 'commands', 'torque'),
    ([str(binaries / 'check-ui-native-safety'), '--target', str(binaries),
      '--output', str(output / 'native-safety'), '--display', 'owned-display',
      '--raylib-root', str(raylib), '--raylib-library', str(raylib / 'lib/libopenpilot-raylib.so')],
      output / 'commands', 'native-safety'),
  ] + [
    (['env', 'DISPLAY=owned-display', str(binaries / f'check-ui-{kind}-adapter'), '--target', str(binaries), '--raylib', str(raylib),
      '--evidence', str(output / f'native-{kind}-adapter')], output / 'commands', f'native-{kind}-adapter')
    for kind in ('model', 'training')
  ]

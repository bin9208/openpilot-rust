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
  assert scripts
  for script in scripts:
    assert (directory / script).is_file(), directory / script
  workspace = tomllib.loads((directory / 'Cargo.toml').read_text())
  assert 'crates/ui-application' in workspace['workspace']['members']

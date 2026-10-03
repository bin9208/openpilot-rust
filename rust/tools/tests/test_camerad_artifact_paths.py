import argparse
import json
from pathlib import Path
import subprocess
import sys
from urllib.parse import quote, unquote

import pytest

import check_camerad_camera_lifecycle as lifecycle
import check_camerad_sensor_lifecycle as sensor


@pytest.mark.parametrize('name', ['error-1-camera:271', 'error-2-sync:6', 'error-camera%3A271'])
def test_camera_failure_evidence_uses_portable_paths_and_preserves_fixture_selector(tmp_path: Path, monkeypatch, name: str) -> None:
  monkeypatch.setattr(lifecycle.subprocess, 'run', lambda *args, **kwargs: subprocess.CompletedProcess(args[0], 0, '{}', ''))
  args = argparse.Namespace(output=tmp_path, qemu=None, fixture=Path('/owned/fixture.so'))
  selector = 'camera:271'
  metadata, records = lifecycle.run(args, 'source', Path('/owned/oracle'), name, ['1', '18', '3'], {'CK_FAIL_OP': selector})
  folder = tmp_path / quote(name, safe='-_.') / 'source'
  assert (folder / 'run.json').is_file()
  assert json.loads((folder / 'run.json').read_text()) == metadata
  assert metadata['environment']['CK_FAIL_OP'] == selector
  assert records == []
  assert unquote(folder.parent.name) == name
  assert not any(character in folder.parent.name for character in '<>:"/\\|?*')


def test_all_sensor_case_artifact_names_are_portable(tmp_path: Path, monkeypatch) -> None:
  binary = tmp_path / 'owned-oracle'
  binary.write_bytes(b'owned fixture')
  output = tmp_path / 'evidence'
  monkeypatch.setattr(sys, 'argv', ['sensor', '--source', str(binary), '--native', str(binary), '--fixture', str(binary), '--output', str(output)])
  monkeypatch.setattr(sensor.subprocess, 'run', lambda *args, **kwargs: subprocess.CompletedProcess(args[0], 0, '{}', ''))
  with pytest.raises(SystemExit) as exit_result:
    sensor.main()
  assert not exit_result.value.code
  report = json.loads((output / 'report.json').read_text())
  assert any('camera:' in row['name'] for row in report['cases'])
  for row in report['cases']:
    for lane in ('source', 'native'):
      path = output / quote(row['name'], safe='-_.') / lane / 'run.json'
      assert path.is_file()
      assert not any(character in str(path.relative_to(output)) for character in '<>:"\\|?*')

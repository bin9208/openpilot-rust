import argparse
import json
from pathlib import Path
import subprocess
from urllib.parse import quote, unquote

import pytest

import check_camerad_camera_lifecycle as lifecycle


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

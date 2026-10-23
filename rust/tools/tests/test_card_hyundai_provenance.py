"""#519 integration controls for the actual staged Hyundai fixture chain.

Run in the Card Actions job after staging, native parameter tests and the full
postcheck. These tests require that real output; they do not replace the source
producer, staging function, native serialization or schema reader with mocks.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import pytest


ROOT = Path(__file__).resolve().parents[3]
CHECKER = ROOT / 'rust/tools/check_card_hyundai_params.py'
SOURCE_PROVENANCE = 'hyundai-source-provenance.json'


@pytest.fixture
def staged() -> Path:
  root = Path(os.environ['HYUNDAI_FIXTURE_DIR'])
  assert root.is_dir(), root
  return root


def postcheck(fixture: Path) -> subprocess.CompletedProcess[str]:
  return subprocess.run([sys.executable, '-P', str(CHECKER), str(fixture)],
    cwd=ROOT, capture_output=True, text=True, timeout=60, check=False)


def shadow_fixture(staged: Path, temporary: Path) -> Path:
  # Keep the real complete source/native binary inputs. Only private JSON
  # metadata copies are damaged; nothing writes through these input links.
  fixture = temporary / 'fixture'
  fixture.mkdir()
  (fixture / 'source-params').symlink_to((staged / 'source-params').resolve(), target_is_directory=True)
  for name in ('detection.json', 'native-params-path.txt', 'provenance.json', SOURCE_PROVENANCE):
    shutil.copyfile(staged / name, fixture / name)
  return fixture


def test_real_staging_retains_both_identities_and_complete_schema_postcheck(staged: Path) -> None:
  before = {name: (staged / name).read_bytes() for name in ('provenance.json', SOURCE_PROVENANCE)}
  generic = json.loads(before['provenance.json'])
  source = json.loads(before[SOURCE_PROVENANCE])
  cases = json.loads((staged / 'detection.json').read_text())
  assert cases and any(case['candidate'] == 'KIA_K5_DL3_24_HEV' for case in cases)
  assert source['runtime_python'] is False
  assert source['detection_cases'] == len(cases)
  assert source['source_parameter_errors'] == []
  assert generic['fixtures'] == str(staged.resolve())
  assert generic['generators'][0] == 'source'
  assert 'rust/tools/card_hyundai_source.py' in generic['source_sha256']
  assert 'rust/tools/stage_card_fixtures.py' in generic['source_sha256']
  for identity in (generic, source):
    assert identity['source_sha256']
    for name, expected in identity['source_sha256'].items():
      assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == expected, name
  result = postcheck(staged)
  assert result.returncode == 0, result.stdout + result.stderr
  receipt = json.loads((staged / 'parameters-result.json').read_text())
  assert receipt['matched_complete_schema'] == len(cases)
  assert receipt['source_parameter_errors'] == []
  assert {name: (staged / name).read_bytes() for name in before} == before


@pytest.mark.parametrize(('damage', 'error'), (
  ('missing-file', 'FileNotFoundError'),
  ('missing-field', "KeyError: 'source_parameter_errors'"),
  ('nonempty-errors', 'AssertionError'),
  ('null-errors', 'AssertionError'),
  ('object-errors', 'AssertionError'),
  ('corrupt-json', 'JSONDecodeError'),
))
def test_real_postcheck_rejects_lost_or_damaged_source_errors(
    staged: Path, tmp_path: Path, damage: str, error: str) -> None:
  fixture = shadow_fixture(staged, tmp_path)
  path = fixture / SOURCE_PROVENANCE
  source = json.loads(path.read_text())
  # Even a plausible generic empty error field must not substitute for the
  # missing/corrupt source record or turn its actual error into success.
  generic = json.loads((fixture / 'provenance.json').read_text())
  generic['source_parameter_errors'] = []
  (fixture / 'provenance.json').write_text(json.dumps(generic))
  if damage == 'missing-file':
    path.unlink()
  elif damage == 'missing-field':
    del source['source_parameter_errors']
    path.write_text(json.dumps(source))
  elif damage == 'corrupt-json':
    path.write_text('{')
  else:
    source['source_parameter_errors'] = {
      'nonempty-errors': ['issue519-controlled-missing-tune'],
      'null-errors': None, 'object-errors': {},
    }[damage]
    path.write_text(json.dumps(source))
  result = postcheck(fixture)
  assert result.returncode != 0, result.stdout + result.stderr
  assert error in result.stderr, result.stdout + result.stderr
  assert not (fixture / 'parameters-result.json').exists()


def test_real_postcheck_still_rejects_complete_schema_difference(staged: Path, tmp_path: Path) -> None:
  fixture = shadow_fixture(staged, tmp_path)
  original = Path((staged / 'native-params-path.txt').read_text())
  native = tmp_path / 'native'
  native.mkdir()
  cases = json.loads((fixture / 'detection.json').read_text())
  # Alter a late real native CarParams record, leaving the exact valid source
  # provenance untouched. The actual reader and complete dict equality must fail.
  from can_source import load
  load()
  from opendbc.car import structs
  changed = len(cases) - 1
  for index in range(len(cases)):
    source = original / f'{index}.bin'
    destination = native / source.name
    if index != changed:
      destination.symlink_to(source.resolve())
      continue
    with structs.CarParams.from_bytes(source.read_bytes()) as message:
      modified = message.as_builder()
      modified.wheelbase += 1.0
      destination.write_bytes(modified.to_bytes())
  (fixture / 'native-params-path.txt').write_text(str(native))
  result = postcheck(fixture)
  assert result.returncode != 0, result.stdout + result.stderr
  assert 'AssertionError' in result.stderr and 'wheelbase' in result.stderr, result.stderr
  assert not (fixture / 'parameters-result.json').exists()

from __future__ import annotations

import json
import argparse
import os
from pathlib import Path
import subprocess

import pytest
import yaml
import check_card_ci
import card_ci_space
from card_qa.ci import ROOT, TOOLS, source_command
from card_qa.runtime_inputs import frames


def test_source_bootstrap_prefers_pinned_binding_over_unbuilt_working_copy(tmp_path: Path) -> None:
  working = tmp_path / 'unbuilt checkout'
  pinned = tmp_path / 'pinned bindings'
  working.mkdir()
  pinned.mkdir()
  (working / 'fixture_binding.py').write_text('raise RuntimeError("unbuilt working-copy binding")\n')
  (pinned / 'fixture_binding.py').write_text('value = "runner binding"\n')
  script = tmp_path / 'oracle.py'
  script.write_text('import fixture_binding\nprint(fixture_binding.value)\n')
  environment = dict(os.environ, PWD=str(working), PYTHONPATH=os.pathsep.join(str(path) for path in
    (pinned, ROOT, ROOT / 'opendbc_repo', TOOLS)))
  result = subprocess.run(source_command(script, [], tmp_path / 'dbc'), cwd=working, env=environment,
    text=True, capture_output=True, check=False)
  assert result.returncode == 0, result.stdout + result.stderr
  assert result.stdout.strip() == 'runner binding'


def test_explicit_corpus_roots_do_not_share_cached_inputs(tmp_path: Path) -> None:
  for index in range(2):
    root = tmp_path / str(index)
    for brand, candidate, filename in (('hyundai', 'GENESIS_G70', 'state.json'), ('tesla', 'TESLA_MODEL_3', 'input.json')):
      directory = root / brand
      directory.mkdir(parents=True)
      frame = {'address': 100 + index, 'data': [index], 'bus': 0}
      row = {'candidate': candidate, 'op': 'runtime', 'steps': [{'packets': [{'frames': [frame]}]}]}
      (directory / filename).write_text(json.dumps([row]))
      assert frames(candidate, 0, root) == [frame]
  with pytest.raises(FileNotFoundError):
    frames('GENESIS_G70', 0, tmp_path / 'absent')


def test_retained_selection_is_bounded_and_preserves_source_bus(tmp_path: Path) -> None:
  frames_ = [{'address': 12, 'data': [1], 'bus': bus} for bus in (4, 6)]
  step = {'packets': [{'frames': frames_}], 'control': [0]}
  cases = [{'candidate': candidate, 'name': candidate, 'op': 'runtime', 'settings': {'source': 'kept'},
            'steps': [step] * 100} for candidate in ('FORD_F_150_MK14', 'FORD_MAVERICK_MK1')]
  retained = tmp_path / 'retained.json'
  retained.write_text(json.dumps(cases))
  output = tmp_path / 'corpus'
  command = source_command(TOOLS / 'stage_card_corpus.py', ['--brand', 'ford', '--input', str(retained),
    '--output', str(output), '--dbc', str(ROOT / 'opendbc_repo/opendbc/dbc')], ROOT / 'opendbc_repo/opendbc/dbc')
  result = subprocess.run(command, text=True, capture_output=True, check=False)
  assert result.returncode == 0, result.stdout + result.stderr
  actual = json.loads((output / 'input.json').read_text())
  assert actual == [{**case, 'steps': case['steps'][:81]} for case in cases]
  assert [frame['bus'] for frame in actual[0]['steps'][0]['packets'][0]['frames']] == [4, 6]


def test_required_ci_generates_fixtures_before_tests_and_checks_full_schemas() -> None:
  workflow = yaml.safe_load((ROOT / '.github/workflows/rust.yml').read_text())
  jobs = workflow['jobs']
  inherited = {'workspace', 'model-memory', 'model-pipelines', 'logger-runtime', 'support-runtime', 'telemetry-runtime',
    'startup-runtime', 'hardware-runtime', 'platform-runtime', 'startup-services', 'sensor-audio', 'gnss-runtime',
    'estimation-runtime', 'ui-connectivity', 'athena-runtime', 'controls-runtime', 'web-upload-timeouts',
    'joystickd-runtime', 'planner-runtime', 'planner-memory', 'radar-runtime', 'radar-arm', 'radar-memory', 'navd-runtime', 'radard-runtime', 'carrot-navi-runtime', 'carrot-navi-arm', 'selfdrive-runtime', 'panda-runtime', 'camera-runtime', 'encoder-runtime', 'xiaoge-runtime', 'xiaoge-memory', 'ui-runtime'}
  assert set(jobs['fast']['needs']) == inherited | {'card-runtime'}
  assert 'test "$CARD" = success' in jobs['fast']['steps'][0]['run']
  for job, test_command in (('workspace', 'cargo test --workspace'), ('card-runtime', 'cargo test -p openpilot-can')):
    scripts = [step.get('run', '') for step in jobs[job]['steps']]
    generation = next(index for index, script in enumerate(scripts) if 'stage_card_fixtures.py' in script)
    testing = next(index for index, script in enumerate(scripts) if test_command in script)
    assert generation < testing
    for checker in ('params', 'state', 'controller'):
      assert any(f'check_card_hyundai_{checker}.py' in script for script in scripts[testing:])
    assert any(step.get('if') == 'always()' and step.get('uses', '').startswith('actions/upload-artifact')
               for step in jobs[job]['steps'])
  scripts = [step.get('run', '') for step in jobs['arm64']['steps']]
  assert any('cargo build -p openpilot-card --features native --bins --examples' in script and
             '--target aarch64-unknown-linux-gnu' in script for script in scripts)
  uploads = [step['with']['path'] for step in jobs['arm64']['steps'] if step.get('uses', '').startswith('actions/upload-artifact')]
  assert any('aarch64-unknown-linux-gnu/release/openpilot-card' in path for path in uploads)


def test_reference_phase_stops_before_brand_outputs_can_cross_disk_floor(tmp_path: Path, monkeypatch) -> None:
  monkeypatch.setattr(card_ci_space.shutil, 'disk_usage', lambda _: type('Usage', (), {'free': 27 * 1024**3})())
  executed = []
  monkeypatch.setattr(check_card_ci, 'run', lambda command, evidence, name: executed.append(name))
  arguments = argparse.Namespace(binaries=tmp_path, dbc=tmp_path, numerics=tmp_path, evidence=tmp_path,
    binding=tmp_path / 'binding')
  with pytest.raises(OSError, match='recover at least 35 GiB'):
    check_card_ci.reference(arguments)
  assert executed == [*check_card_ci.SHARED, 'git_source', 'carlog', 'common']

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


def _assert_required_native_arm_binding(workflow: dict, native: dict) -> None:
  jobs, native_jobs = workflow['jobs'], native['jobs']
  trusted = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}"
  denial = 'test "${SOURCE_ALLOWED:-}" = true || { echo \'Fork PR source denied on persistent bin-actions\' >&2; exit 1; }'
  results = {'PLANNER': 'planner-runtime', 'CARROT_MAN': 'carrot-man-runtime', 'WEBRTC': 'webrtc-runtime',
    'WEBCAM': 'webcam-runtime', 'XIAOGE': 'xiaoge-runtime', 'RADARD': 'radard-runtime',
    'RADAR_ARM': 'radar-arm', 'CARROT_NAVI_ARM': 'carrot-navi-arm'}
  assert jobs['native-arm'] == {'uses': './.github/workflows/rust-native-arm.yml'}
  fast = jobs['fast']
  assert fast['if'] == '${{ always() }}' and 'continue-on-error' not in fast
  step = fast['steps'][0]
  assert 'if' not in step and 'continue-on-error' not in step
  assert step['env']['SOURCE_ALLOWED'] == trusted
  assert step['env']['NATIVE_ARM'] == '${{ needs.native-arm.result }}'
  assert step['run'].splitlines()[0] == denial
  assert step['run'].splitlines().count('test "$NATIVE_ARM" = success') == 1
  assert set(native_jobs) == {*results.values(), 'native-arm-checks'}
  for name in results.values():
    job = native_jobs[name]
    assert job['runs-on'] == 'ubuntu-24.04-arm'
    assert 'if' not in job and 'continue-on-error' not in job and 'needs' not in job
    assert job['env']['CI296_NATIVE_ARM'] == '1' and job['env']['CI296_NATIVE_ARM_CALLER'] == 'rust.yml'
  gate = native_jobs['native-arm-checks']
  assert gate['needs'] == list(results.values())
  assert gate['if'] == '${{ always() }}' and 'continue-on-error' not in gate
  assert gate['runs-on'] == 'ubuntu-24.04' and gate['timeout-minutes'] == 2
  assert len(gate['steps']) == 1
  step = gate['steps'][0]
  assert 'if' not in step and 'continue-on-error' not in step
  assert step['env'] == {'SOURCE_ALLOWED': trusted, **{
    variable: '${{ needs.' + name + '.result }}' for variable, name in results.items()}}
  assert step['run'].splitlines() == [denial, *[f'test "${variable}" = success' for variable in results]]
  for name, label in (('radar-arm', 'rust RadarCAN native ARM functional validation'),
                      ('carrot-navi-arm', 'rust Carrot Navi native ARM build')):
    assert jobs[name] == {'name': label, 'needs': ['native-arm'], 'if': '${{ always() }}',
      'runs-on': 'ubuntu-24.04', 'timeout-minutes': 2, 'steps': [{'name': 'Require native ARM family result',
        'env': {'SOURCE_ALLOWED': trusted, 'NATIVE_ARM': '${{ needs.native-arm.result }}'},
        'run': denial + '\ntest "$NATIVE_ARM" = success\n'}]}


def test_required_ci_generates_fixtures_before_tests_and_checks_full_schemas() -> None:
  workflow = yaml.safe_load((ROOT / '.github/workflows/rust.yml').read_text())
  jobs = workflow['jobs']
  inherited = {'workspace', 'model-llvm20', 'model-memory', 'model-pipelines', 'model-family-retirement', 'logger-runtime', 'support-runtime', 'telemetry-runtime',
    'startup-runtime', 'hardware-runtime', 'platform-runtime', 'startup-services', 'sensor-audio', 'gnss-runtime',
    'estimation-runtime', 'ui-connectivity', 'athena-runtime', 'controls-runtime', 'web-upload-timeouts',
    'joystickd-runtime', 'planner-runtime', 'planner-memory', 'radar-runtime', 'radar-arm', 'radar-memory', 'navd-runtime', 'radard-runtime',
    'carrot-navi-runtime', 'carrot-navi-arm', 'selfdrive-runtime', 'panda-runtime', 'camera-runtime', 'encoder-runtime',
    'xiaoge-runtime', 'xiaoge-memory', 'ui-runtime', 'carrot-man-runtime'}
  required = inherited | {'card-runtime', 'webrtc-runtime', 'webcam-runtime', 'dbc-darwin'}
  assert len(required) == 41
  assert set(jobs['fast']['needs']) == required | {'native-arm'}
  assert len(jobs['fast']['needs']) == 42
  native = yaml.safe_load((ROOT / '.github/workflows/rust-native-arm.yml').read_text())
  _assert_required_native_arm_binding(workflow, native)
  assert 'test "$CARD" = success' in jobs['fast']['steps'][0]['run']
  assert jobs['fast']['steps'][0]['env']['MODEL_FAMILY'] == '${{ needs.model-family-retirement.result }}'
  assert 'test "$MODEL_FAMILY" = success' in jobs['fast']['steps'][0]['run']
  for job, test_command in (('workspace', 'cargo test --workspace'), ('card-runtime', 'cargo test -p openpilot-can')):
    scripts = [step.get('run', '') for step in jobs[job]['steps']]
    generation = next(index for index, script in enumerate(scripts) if 'stage_card_fixtures.py' in script)
    testing = next(index for index, script in enumerate(scripts) if test_command in script)
    assert generation < testing
    for checker in ('params', 'state', 'controller'):
      assert any(f'check_card_hyundai_{checker}.py' in script for script in scripts[testing:])
    upload_condition = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always()) }}"
    assert any(step.get('if') == upload_condition and step.get('uses', '').startswith('actions/upload-artifact')
               for step in jobs[job]['steps'])
  scripts = [step.get('run', '') for step in jobs['arm64']['steps']]
  assert any('cargo build -p openpilot-card --features native --bins --examples' in script and
             '--target aarch64-unknown-linux-gnu' in script for script in scripts)
  uploads = [step['with']['path'] for step in jobs['arm64']['steps'] if step.get('uses', '').startswith('actions/upload-artifact')]
  assert any('aarch64-unknown-linux-gnu/release/openpilot-card' in path for path in uploads)


@pytest.mark.parametrize(('scope', 'path', 'value'), [
  ('main', ('jobs', 'native-arm', 'uses'), './.github/workflows/foreign.yml'),
  ('main', ('jobs', 'native-arm', 'if'), '${{ false }}'),
  ('main', ('jobs', 'fast', 'steps', 0, 'env', 'NATIVE_ARM'), '${{ needs.radar-arm.result }}'),
  ('main', ('jobs', 'radar-arm', 'needs'), []),
  ('main', ('jobs', 'carrot-navi-arm', 'if'), '${{ false }}'),
  ('native', ('jobs', 'radar-arm', 'env', 'CI296_NATIVE_ARM_CALLER'), 'foreign.yml'),
  ('native', ('jobs', 'native-arm-checks', 'needs'), ['radar-arm', 'carrot-navi-arm']),
  ('native', ('jobs', 'native-arm-checks', 'if'), '${{ success() }}'),
  ('native', ('jobs', 'native-arm-checks', 'steps', 0, 'env', 'RADAR_ARM'), '${{ needs.radard-runtime.result }}'),
  ('native', ('jobs', 'native-arm-checks', 'steps', 0, 'run'), 'exit 0\n'),
])
def test_required_native_arm_binding_rejects_missing_skipped_or_rebound_results(scope, path, value) -> None:
  workflow = yaml.safe_load((ROOT / '.github/workflows/rust.yml').read_text())
  native = yaml.safe_load((ROOT / '.github/workflows/rust-native-arm.yml').read_text())
  target = workflow if scope == 'main' else native
  for key in path[:-1]:
    target = target[key]
  target[path[-1]] = value
  with pytest.raises(AssertionError):
    _assert_required_native_arm_binding(workflow, native)


def test_reference_phase_stops_before_brand_outputs_can_cross_disk_floor(tmp_path: Path, monkeypatch) -> None:
  monkeypatch.setattr(card_ci_space.shutil, 'disk_usage', lambda _: type('Usage', (), {'free': 27 * 1024**3})())
  executed = []
  monkeypatch.setattr(check_card_ci, 'run', lambda command, evidence, name: executed.append(name))
  arguments = argparse.Namespace(binaries=tmp_path, dbc=tmp_path, numerics=tmp_path, evidence=tmp_path,
    binding=tmp_path / 'binding')
  with pytest.raises(OSError, match='recover at least 35 GiB'):
    check_card_ci.reference(arguments)
  assert executed == [*check_card_ci.SHARED, 'git_source', 'carlog', 'common']

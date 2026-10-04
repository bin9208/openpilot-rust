from __future__ import annotations

import argparse
from pathlib import Path

import pytest

import check_radarcan_ci as ci
from radarcan_runtime_capture import recorded_environment


def arguments(tmp_path: Path) -> argparse.Namespace:
  return argparse.Namespace(probe=tmp_path / 'probe', daemon=tmp_path / 'daemon', binding=tmp_path / 'binding',
    dbc=tmp_path / 'dbc', numerics=tmp_path / 'numerics', evidence=tmp_path / 'evidence', runner=None, sysroot=None)


def test_reference_requires_capacity_before_each_complete_gate(tmp_path, monkeypatch):
  observed = []
  monkeypatch.setattr(ci, 'require_space', lambda _path, growth: observed.append(('guard', growth)))

  def run(command, _evidence, name):
    assert observed[-1] == ('guard', 1024**3)
    assert command[1] == '-P'
    observed.append(('gate', name))

  monkeypatch.setattr(ci, 'run', run)
  ci.reference(arguments(tmp_path))
  assert [name for kind, name in observed if kind == 'gate'] == [
    'common', 'base', 'decoder', 'hyundai', 'cluster', 'runtime', 'constructors']


def test_reference_stops_before_next_gate_when_capacity_falls(tmp_path, monkeypatch):
  gates = []
  guards = 0

  def reserve(_path, _growth):
    nonlocal guards
    guards += 1
    if guards == 4:
      raise OSError('fixture exhausted capacity')

  monkeypatch.setattr(ci, 'require_space', reserve)
  monkeypatch.setattr(ci, 'run', lambda _command, _evidence, name: gates.append(name))
  with pytest.raises(OSError):
    ci.reference(arguments(tmp_path))
  assert gates == ['common', 'base', 'decoder']


def test_ipc_stages_fresh_corpus_then_keeps_all_three_real_surfaces(tmp_path, monkeypatch):
  observed = []
  monkeypatch.setattr(ci, 'require_space', lambda _path, growth: observed.append(('guard', growth)))

  def run(command, _evidence, name):
    expected = 64 * 1024**2 if name == 'stage-runtime' else 512 * 1024**2
    assert observed[-1] == ('guard', expected)
    assert command[1] == '-P'
    if name == 'lifecycle':
      assert '--lifecycle' in command
    elif name != 'stage-runtime':
      assert Path(command[command.index('--cases') + 1]).name == name + '.json'
    observed.append(('gate', name))

  monkeypatch.setattr(ci, 'run', run)
  ci.ipc(arguments(tmp_path))
  assert [name for kind, name in observed if kind == 'gate'] == ['stage-runtime', 'normal', 'joined', 'lifecycle']


def test_cross_reference_preserves_runner_and_sysroot_on_all_seven_gates(tmp_path, monkeypatch):
  args = arguments(tmp_path)
  args.runner, args.sysroot = Path('/fixture/qemu-aarch64'), Path('/fixture/arm sysroot')
  monkeypatch.setattr(ci, 'require_space', lambda _path, _growth: None)
  commands = []
  monkeypatch.setattr(ci, 'run', lambda command, _evidence, _name: commands.append(command))
  ci.reference(args)
  assert len(commands) == 7
  for command in commands:
    assert command[-4:] == ['--runner', '/fixture/qemu-aarch64', '--sysroot', '/fixture/arm sysroot']


def test_uploaded_invocations_keep_runtime_controls_without_runner_credentials():
  environment = {'OPENPILOT_PREFIX': 'fixture', 'PARAMS_ROOT': '/fixture/params', 'REPLAY': '1',
    'PYTHONPATH': '/fixture/source', 'LD_LIBRARY_PATH': '/fixture/libraries',
    'GITHUB_TOKEN': 'fixture-secret', 'ACTIONS_RUNTIME_TOKEN': 'fixture-upload-secret'}
  before = environment.copy()
  assert recorded_environment(environment) == {
    key: value for key, value in environment.items() if key not in ('GITHUB_TOKEN', 'ACTIONS_RUNTIME_TOKEN')}
  assert environment == before

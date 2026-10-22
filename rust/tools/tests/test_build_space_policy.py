from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

import pytest

import build_plannerd_acados
import card_ci_space
import check_encoder_target
import check_locationd_native
import check_paramsd_native
import check_plannerd_ci
import run_xiaoge_opencv_qa


@pytest.mark.parametrize('growth', [0, 128 * 1024**2, 2**64 - 1])
def test_ci_space_admits_exact_growth_without_a_fixed_floor(tmp_path: Path, monkeypatch, growth: int) -> None:
  monkeypatch.setattr(card_ci_space.shutil, 'disk_usage', lambda _: SimpleNamespace(free=growth))
  assert card_ci_space.require_space(tmp_path / 'not-created', growth) == {
    'free_bytes': growth, 'required_bytes': growth, 'growth_bytes': growth}
  if growth:
    monkeypatch.setattr(card_ci_space.shutil, 'disk_usage', lambda _: SimpleNamespace(free=growth - 1))
    with pytest.raises(OSError, match='insufficient space for estimated growth'):
      card_ci_space.require_space(tmp_path, growth)


@pytest.mark.parametrize(('guard', 'growth'), [
  (build_plannerd_acados.disk_guard, 128 * 1024**2),
  (check_locationd_native.disk, 2 * 1024**3),
  (check_paramsd_native.disk, 2 * 1024**3),
])
def test_native_build_guards_keep_their_growth_allowance(tmp_path: Path, monkeypatch, guard, growth: int) -> None:
  monkeypatch.setattr(card_ci_space.shutil, 'disk_usage', lambda _: SimpleNamespace(free=growth))
  assert guard(tmp_path) == growth
  monkeypatch.setattr(card_ci_space.shutil, 'disk_usage', lambda _: SimpleNamespace(free=growth - 1))
  with pytest.raises(RuntimeError, match='growth'):
    guard(tmp_path)


@pytest.mark.parametrize('module', [card_ci_space, run_xiaoge_opencv_qa])
def test_negative_cli_growth_is_rejected_before_disk_access(tmp_path: Path, monkeypatch, module) -> None:
  arguments = (['--path', str(tmp_path)] if module is card_ci_space else
    ['--evidence', str(tmp_path), '--phase', 'fixture', '--cwd', str(tmp_path)])
  monkeypatch.setattr(sys, 'argv', ['guard', *arguments, '--growth-mib', '-1', *(
    [] if module is card_ci_space else ['--', 'fixture-command'])])
  monkeypatch.setattr(module.shutil, 'disk_usage', lambda _: pytest.fail('negative growth must fail before disk access'))
  with pytest.raises(SystemExit) as exited:
    module.main()
  assert exited.value.code == 2


@pytest.mark.parametrize('growth', [0, 256 * 1024**2, 2**64 - 1])
def test_encoder_guard_records_actual_requirement_and_rejects_shortfall(tmp_path: Path, monkeypatch, growth: int) -> None:
  monkeypatch.setattr(check_encoder_target.shutil, 'disk_usage', lambda _: SimpleNamespace(free=growth))
  check_encoder_target.disk_guard(tmp_path, growth)
  record = json.loads((tmp_path / 'disk-guards.jsonl').read_text())
  assert record['free_bytes'] == record['required_bytes'] == record['growth_bytes'] == growth
  assert 'floor_bytes' not in record
  if growth:
    monkeypatch.setattr(check_encoder_target.shutil, 'disk_usage', lambda _: SimpleNamespace(free=growth - 1))
    with pytest.raises(check_encoder_target.TargetFailure, match=f'required={growth}; free={growth - 1}'):
      check_encoder_target.disk_guard(tmp_path, growth)


@pytest.mark.parametrize('free', [512 * 1024**2 - 1, 512 * 1024**2])
def test_planner_checks_admit_only_when_growth_fits(tmp_path: Path, monkeypatch, free: int) -> None:
  monkeypatch.setattr(check_plannerd_ci.shutil, 'disk_usage', lambda _: SimpleNamespace(free=free))
  launched = []

  def run(command, **kwargs):
    launched.append(command)
    return subprocess.CompletedProcess(command, 0)

  monkeypatch.setattr(check_plannerd_ci.subprocess, 'run', run)
  output = tmp_path / 'checks'
  checks = (check_plannerd_ci.Check('fixture', tmp_path / 'fixture.py'),)
  if free < 512 * 1024**2:
    with pytest.raises(check_plannerd_ci.InsufficientSpace, match='512 MiB estimated growth'):
      check_plannerd_ci.run_checks(checks, output)
    assert not launched
  else:
    check_plannerd_ci.run_checks(checks, output)
    assert len(launched) == 1
    assert json.loads((output / 'commands.json').read_text())[0]['free_before'] == free


@pytest.mark.parametrize(('growth_mib', 'shortfall'), [(0, 0), (128, 0), (128, 1)])
def test_qa_wrapper_records_growth_and_stops_before_launch_on_shortfall(
  tmp_path: Path, monkeypatch, growth_mib: int, shortfall: int,
) -> None:
  growth = growth_mib * 1024**2
  free = growth - shortfall
  monkeypatch.setattr(run_xiaoge_opencv_qa.shutil, 'disk_usage', lambda _: SimpleNamespace(free=free))
  monkeypatch.setattr(sys, 'argv', ['qa', '--evidence', str(tmp_path), '--phase', 'fixture',
    '--cwd', str(tmp_path), '--growth-mib', str(growth_mib), '--', 'fixture-command'])
  launched = []

  def run(command, **kwargs):
    launched.append(command)
    return subprocess.CompletedProcess(command, 0)

  monkeypatch.setattr(run_xiaoge_opencv_qa.subprocess, 'run', run)
  if shortfall:
    with pytest.raises(run_xiaoge_opencv_qa.InsufficientSpace, match=f'required={growth}; free={free}'):
      run_xiaoge_opencv_qa.main()
    assert not launched
    assert not (tmp_path / 'fixture.exit.json').exists()
  else:
    with pytest.raises(SystemExit) as exited:
      run_xiaoge_opencv_qa.main()
    assert exited.value.code == 0
    assert launched == [['fixture-command']]
  record = json.loads((tmp_path / 'fixture.command.json').read_text())
  assert record['required_bytes'] == record['growth_bytes'] == growth
  assert record['free_bytes'] == free

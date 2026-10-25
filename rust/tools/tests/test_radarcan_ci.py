from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from types import SimpleNamespace

import pytest

import check_radarcan_ci as ci
from radarcan_runtime_capture import fd_targets, read_producer_completion, recorded_environment, wait_for_producer_exit
from radarcan_runtime_producer import publish_completion


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


def test_fd_capture_records_link_closed_after_listing(tmp_path, monkeypatch):
  fd_dir = tmp_path / 'proc' / '123' / 'fd'
  fd_dir.mkdir(parents=True)
  (fd_dir / '3').symlink_to('/fixture/live')
  (fd_dir / '4').symlink_to('/fixture/closed')
  original_readlink = os.readlink

  def close_then_readlink(path):
    if Path(path).name == '4':
      Path(path).unlink()
    return original_readlink(path)

  monkeypatch.setattr(os, 'readlink', close_then_readlink)
  targets, closed = fd_targets(fd_dir)
  assert targets == {'3': '/fixture/live'}
  assert closed == ['4']


def test_fd_capture_does_not_mask_missing_process_or_permission(tmp_path, monkeypatch):
  fd_dir = tmp_path / 'proc' / '123' / 'fd'
  with pytest.raises(FileNotFoundError):
    fd_targets(fd_dir)

  fd_dir.mkdir(parents=True)
  (fd_dir / '3').symlink_to('/fixture/live')

  def denied(_path):
    raise PermissionError('fixture denied')

  monkeypatch.setattr(os, 'readlink', denied)
  with pytest.raises(PermissionError, match='fixture denied'):
    fd_targets(fd_dir)


def test_fd_capture_does_not_mask_directory_disappearance(tmp_path, monkeypatch):
  fd_dir = tmp_path / 'proc' / '123' / 'fd'
  fd_dir.mkdir(parents=True)
  (fd_dir / '3').symlink_to('/fixture/live')
  original_readlink = os.readlink

  def remove_directory_then_readlink(path):
    Path(path).unlink()
    fd_dir.rmdir()
    return original_readlink(path)

  monkeypatch.setattr(os, 'readlink', remove_directory_then_readlink)
  with pytest.raises(FileNotFoundError):
    fd_targets(fd_dir)


def completion_proof(tmp_path):
  inputs = tmp_path / 'inputs.json'
  inputs.write_bytes(b'[{"can":["01"],"carState":"02"}]\n')
  output = tmp_path / 'producer.json'
  output.write_text(json.dumps({'pid': os.getpid(), 'final_ack_ns': 3, 'evidence_done_ns': 4,
    'inputs_sha256': hashlib.sha256(inputs.read_bytes()).hexdigest(), 'rows': [{'send_started_ns': 1}]}) + '\n')
  return output


def test_producer_completion_is_atomic_without_waiting_for_python_teardown(tmp_path, monkeypatch):
  output = completion_proof(tmp_path)
  producer = SimpleNamespace(pid=os.getpid(), poll=lambda: None)
  assert read_producer_completion(producer, output) is None
  original_replace = Path.replace
  renamed = []

  def checked_replace(pending, complete):
    assert pending == tmp_path / 'producer-complete.pending'
    assert complete == tmp_path / 'producer-complete.json'
    assert not complete.exists()
    assert read_producer_completion(producer, output) is None
    assert json.loads(pending.read_bytes())['producer_sha256'] == hashlib.sha256(output.read_bytes()).hexdigest()
    renamed.append(pending)
    return original_replace(pending, complete)

  monkeypatch.setattr(Path, 'replace', checked_replace)
  publish_completion(output)
  marker, proof = read_producer_completion(producer, output)
  assert len(renamed) == 1 and not renamed[0].exists()
  assert marker['pid'] == producer.pid and marker['completed_ns'] >= proof['evidence_done_ns']
  assert proof == json.loads(output.read_bytes())
  assert producer.poll() is None


@pytest.mark.parametrize('fault', ['exit-without-marker', 'nonzero', 'pid', 'partial', 'proof', 'inputs'])
def test_producer_completion_rejects_missing_stale_partial_or_mutated_success(tmp_path, fault):
  output = completion_proof(tmp_path)
  producer = SimpleNamespace(pid=os.getpid(), poll=lambda: 0 if fault == 'exit-without-marker' else 7 if fault == 'nonzero' else None)
  if fault not in ('exit-without-marker', 'nonzero'):
    publish_completion(output)
    complete = tmp_path / 'producer-complete.json'
    if fault == 'pid':
      marker = json.loads(complete.read_bytes())
      marker['pid'] += 1
      complete.write_text(json.dumps(marker))
    elif fault == 'partial':
      complete.write_bytes(b'{"schema":1,')
    elif fault == 'proof':
      proof = json.loads(output.read_bytes())
      proof['rows'][0]['send_started_ns'] += 1
      output.write_text(json.dumps(proof))
    elif fault == 'inputs':
      inputs = tmp_path / 'inputs.json'
      inputs.write_bytes(inputs.read_bytes().replace(b'01', b'03'))
  with pytest.raises((AssertionError, json.JSONDecodeError)):
    read_producer_completion(producer, output)


def test_producer_completion_publish_failure_never_exposes_success(tmp_path, monkeypatch):
  output = completion_proof(tmp_path)

  def failed_replace(_pending, _complete):
    raise OSError('fixture publication failure')

  monkeypatch.setattr(Path, 'replace', failed_replace)
  with pytest.raises(OSError, match='fixture publication failure'):
    publish_completion(output)
  assert (tmp_path / 'producer-complete.pending').is_file()
  assert not (tmp_path / 'producer-complete.json').exists()
  assert read_producer_completion(SimpleNamespace(pid=os.getpid(), poll=lambda: None), output) is None


@pytest.mark.parametrize('exit_code', [0, 7, 'timeout'])
def test_producer_exit_wait_requires_actual_zero_without_kill_acceptance(exit_code):
  calls = []

  def wait(timeout):
    calls.append(timeout)
    if exit_code == 'timeout':
      raise subprocess.TimeoutExpired('fixture producer', timeout)
    return exit_code

  def kill():
    pytest.fail('producer teardown must not be killed and accepted')

  producer = SimpleNamespace(wait=wait, kill=kill)
  if exit_code == 0:
    assert wait_for_producer_exit(producer) > 0
  else:
    with pytest.raises(subprocess.TimeoutExpired if exit_code == 'timeout' else AssertionError):
      wait_for_producer_exit(producer)
  assert calls == [3]

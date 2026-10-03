from __future__ import annotations

import json
import ast
import gc
import io
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import uuid

import pytest

from encoder_outcomes import check_outcome, stop_cleanly


def test_already_exited_failure_is_recorded_and_rejected(tmp_path: Path) -> None:
  with subprocess.Popen([sys.executable, '-c', 'raise SystemExit(23)']) as child:
    assert child.wait(timeout=3) == 23
    receipt = tmp_path / 'shutdown.json'
    with pytest.raises(AssertionError, match='23'):
      stop_cleanly(child, receipt)
    assert json.loads(receipt.read_text()) == {'pid': child.pid, 'signaled': False, 'timed_out': False, 'returncode': 23}


def test_runtime_peer_checks_an_already_failed_child_and_cleans_resources(tmp_path: Path) -> None:
  source = Path(__file__).parents[1] / 'check_encoder_runtime.py'
  tree = ast.parse(source.read_text())
  peer_class = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'Peer')
  namespace = {'Path': Path, 'json': json, 'signal': signal, 'gc': gc, 'shutil': shutil, 'stop_cleanly': stop_cleanly}
  exec(compile(ast.Module(body=[peer_class], type_ignores=[]), str(source), 'exec'), namespace)
  peer = namespace['Peer'].__new__(namespace['Peer'])
  peer.directory = tmp_path
  peer.raw, peer.output = io.BytesIO(), io.BytesIO()
  peer.rows, peer.inputs, peer.sockets = {}, [], {}
  peer.server = object()
  peer.shm = tmp_path / 'shm'
  peer.shm.mkdir()
  peer.prefix = 'encoder-owned-test-' + uuid.uuid4().hex
  peer.drain = lambda: None
  with subprocess.Popen([sys.executable, '-c', 'raise SystemExit(23)']) as child:
    peer.process = child
    assert child.wait(timeout=3) == 23
    with pytest.raises(AssertionError, match='23'):
      peer.close()
  assert peer.raw.closed and peer.output.closed
  assert not peer.shm.exists()
  assert json.loads((tmp_path / 'shutdown.json').read_text())['returncode'] == 23


def test_live_child_shutdown_records_the_observed_exit(tmp_path: Path) -> None:
  program = 'import signal,sys; signal.signal(signal.SIGTERM, lambda *_: sys.exit(0)); print("ready",flush=True); signal.pause()'
  with subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE, text=True) as child:
    try:
      assert child.stdout.readline() == 'ready\n'
      receipt = tmp_path / 'shutdown.json'
      stop_cleanly(child, receipt)
      assert json.loads(receipt.read_text()) == {'pid': child.pid, 'signaled': True, 'timed_out': False, 'returncode': 0}
    finally:
      if child.poll() is None:
        child.kill()
        child.wait(timeout=3)


def test_unresponsive_owned_child_is_reaped_and_rejected(tmp_path: Path) -> None:
  program = 'import signal; signal.signal(signal.SIGTERM, signal.SIG_IGN); print("ready",flush=True); signal.pause()'
  with subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE, text=True) as child:
    try:
      assert child.stdout.readline() == 'ready\n'
      receipt = tmp_path / 'shutdown.json'
      with pytest.raises(AssertionError, match='timed out'):
        stop_cleanly(child, receipt, timeout=0.01)
      assert child.poll() == -signal.SIGKILL
      assert json.loads(receipt.read_text())['timed_out'] is True
    finally:
      if child.poll() is None:
        child.kill()
        child.wait(timeout=3)


def test_expected_contract_requires_the_exit_code_and_diagnostic() -> None:
  check_outcome(-signal.SIGABRT, 'VIDIOC_QBUF failed', (-signal.SIGABRT, 'VIDIOC_QBUF failed'))
  check_outcome(1, 'Error: Contract("wrong output")', (1, 'wrong output'))
  with pytest.raises(AssertionError):
    check_outcome(-signal.SIGSEGV, 'VIDIOC_QBUF failed', (-signal.SIGABRT, 'VIDIOC_QBUF failed'))
  with pytest.raises(AssertionError):
    check_outcome(-signal.SIGABRT, 'unrelated assertion', (-signal.SIGABRT, 'VIDIOC_QBUF failed'))


@pytest.mark.parametrize(
  'diagnostic',
  [
    'ERROR: AddressSanitizer: heap-use-after-free',
    'LeakSanitizer: detected memory leaks',
    'runtime error: misaligned address',
    'UndefinedBehaviorSanitizer: undefined-behavior',
  ],
)
def test_sanitizer_failure_never_satisfies_expected_contract(diagnostic: str) -> None:
  with pytest.raises(AssertionError, match='sanitizer'):
    check_outcome(1, 'wrong output\n' + diagnostic, (1, 'wrong output'))
  with pytest.raises(AssertionError, match='sanitizer'):
    check_outcome(0, diagnostic)

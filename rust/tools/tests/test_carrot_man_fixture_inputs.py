from dataclasses import dataclass
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import pytest

from openpilot.cereal import log, messaging

from carrot_man_fixture_inputs import InputPublisher


@dataclass(frozen=True, slots=True)
class Inputs:
  network: str = 'none'
  can_error: bool = False


def test_inputs_continue_while_the_fixture_caller_is_blocked(tmp_path: Path) -> None:
  with tempfile.TemporaryDirectory(prefix='msgq_carrot_inputs_', dir='/dev/shm') as directory:
    previous = os.environ.get('OPENPILOT_PREFIX')
    os.environ['OPENPILOT_PREFIX'] = Path(directory).name.removeprefix('msgq_')
    publisher = messaging.PubMaster(['deviceState', 'carState', 'selfdriveState', 'carControl', 'gpsLocationExternal', 'modelV2'])
    stream = messaging.sub_sock('carState', conflate=True, timeout=1000)
    inputs = InputPublisher(publisher, Inputs(), tmp_path)
    try:
      assert stream.receive() is not None
      blocked_at = time.monotonic_ns()
      threading.Event().wait(.25)
      raw = stream.receive()
      assert raw is not None
      with log.Event.from_bytes(raw) as event:
        assert event.logMonoTime > blocked_at + 150_000_000
    finally:
      try:
        inputs.close()
      finally:
        stream = None
        publisher = None
        if previous is None:
          os.environ.pop('OPENPILOT_PREFIX')
        else:
          os.environ['OPENPILOT_PREFIX'] = previous
    receipt = json.loads((tmp_path / 'input-publications.json').read_text())
    assert receipt['joined'] and receipt['failure'] is None
    assert len(receipt['publications']) > 1


def test_publisher_failure_reaches_the_caller_after_the_thread_is_joined(tmp_path: Path) -> None:
  with tempfile.TemporaryDirectory(prefix='msgq_carrot_inputs_', dir='/dev/shm') as directory:
    previous = os.environ.get('OPENPILOT_PREFIX')
    os.environ['OPENPILOT_PREFIX'] = Path(directory).name.removeprefix('msgq_')
    publisher = messaging.PubMaster(['deviceState'])
    inputs = InputPublisher(publisher, Inputs(network='unsupported-enum'), tmp_path)
    try:
      assert inputs.stopped.wait(1)
    finally:
      try:
        with pytest.raises(RuntimeError, match='owned input publisher failed') as error:
          inputs.close()
        assert error.value.__cause__ is not None
      finally:
        publisher = None
        if previous is None:
          os.environ.pop('OPENPILOT_PREFIX')
        else:
          os.environ['OPENPILOT_PREFIX'] = previous
    receipt = json.loads((tmp_path / 'input-publications.json').read_text())
    assert receipt['joined'] and receipt['failure']

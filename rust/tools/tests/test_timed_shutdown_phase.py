import json
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time

from timed_shutdown_phase import wait_original_gps_sleep


def test_source_signal_waits_for_sleep_after_an_earlier_ready_marker(tmp_path: Path) -> None:
  script = 'import sys,time; print("command-recorded",flush=True); sys.stdin.read(1); time.sleep(10)'
  process = subprocess.Popen([sys.executable, '-c', script], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, text=True)
  marker = tmp_path / 'sleep-readiness.json'
  released = threading.Event()

  def release() -> None:
    deadline = time.monotonic() + 3
    while not marker.exists() and time.monotonic() < deadline:
      time.sleep(.002)
    assert marker.exists()
    assert process.stdin is not None
    released.set()
    process.stdin.write('g')
    process.stdin.flush()

  thread = threading.Thread(target=release)
  try:
    assert process.stdout is not None
    assert process.stdout.readline().strip() == 'command-recorded'
    thread.start()
    phase = wait_original_gps_sleep(process, tmp_path)
    assert released.is_set()
    assert phase['wchan'] == 'hrtimer_nanosleep'
    observations = json.loads(marker.read_text())
    assert observations[0]['state']['wchan'] != 'hrtimer_nanosleep'
    assert observations[-1]['state']['wchan'] == 'hrtimer_nanosleep'
    started = time.monotonic()
    process.send_signal(signal.SIGINT)
    assert process.wait(timeout=2) == -signal.SIGINT
    assert time.monotonic() - started < 2
  finally:
    if process.poll() is None:
      process.kill()
      process.wait(timeout=3)
    if thread.ident is not None:
      thread.join(timeout=3)
      assert not thread.is_alive()
    if process.stdin is not None:
      process.stdin.close()
    if process.stdout is not None:
      process.stdout.close()

import json
from pathlib import Path
import subprocess
import sys
import textwrap
import time

import pytest

from loggerd_peer import Peer


@pytest.mark.parametrize('mode', ('sleep', 'zero-ppoll', 'fd-ppoll', 'pipe-read', 'futex', 'exited'))
def test_idle_barrier_accepts_only_source_sleep_or_zero_fd_ppoll(tmp_path: Path, mode: str) -> None:
  script = textwrap.dedent(r'''
    import ctypes, os, sys, threading, time
    class PollFd(ctypes.Structure):
      _fields_ = [('fd', ctypes.c_int), ('events', ctypes.c_short), ('revents', ctypes.c_short)]
    library = ctypes.CDLL(None)
    timeout = (ctypes.c_long * 2)(3, 0)
    reader, writer = os.pipe()
    descriptor = PollFd(reader, 1, 0)
    lock = threading.Lock()
    lock.acquire()
    waits = {
      'sleep': lambda: time.sleep(3),
      'zero-ppoll': lambda: library.ppoll(None, ctypes.c_ulong(0), ctypes.byref(timeout), None),
      'fd-ppoll': lambda: library.ppoll(ctypes.byref(descriptor), ctypes.c_ulong(1), ctypes.byref(timeout), None),
      'pipe-read': lambda: os.read(reader, 1),
      'futex': lock.acquire,
      'exited': lambda: None,
    }
    os.write(1, b'ready\n')
    waits[sys.argv[1]]()
  ''')
  command = [sys.executable, '-c', script, mode]
  with (tmp_path / 'stderr.log').open('wb') as stderr, subprocess.Popen(command, stdout=subprocess.PIPE, stderr=stderr) as child:
    try:
      assert child.stdout is not None and child.stdout.readline() == b'ready\n'
      peer = Peer.__new__(Peer)
      peer.root, peer.process = tmp_path, child
      if mode == 'exited':
        assert child.wait(timeout=2) == 0
        with pytest.raises(RuntimeError):
          peer.await_idle(timeout=.04)
        record = {'mode': mode, 'exit': child.returncode, 'observer': 'process exit rejected'}
      else:
        proc = Path('/proc') / str(child.pid)
        deadline = time.monotonic() + 2
        while True:
          channel = (proc / 'wchan').read_text().strip()
          syscall = (proc / 'syscall').read_text().strip()
          if channel not in ('', '0') and syscall not in ('', 'running'):
            break
          assert child.poll() is None and time.monotonic() < deadline, (channel, syscall)
          time.sleep(.001)
        record = {'mode': mode, 'wchan': channel, 'syscall': syscall}
        print(json.dumps(record))
        if mode in ('sleep', 'zero-ppoll'):
          peer.await_idle(timeout=.04)
          record['observer'] = 'idle accepted'
        else:
          with pytest.raises(TimeoutError):
            peer.await_idle(timeout=.04)
          record['observer'] = 'non-idle timeout preserved'
      (tmp_path / 'observation.json').write_text(json.dumps(record, indent=2))
      print(json.dumps(record))
    finally:
      if child.poll() is None:
        child.terminate()
        child.wait(timeout=2)

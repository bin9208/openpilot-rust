from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import subprocess
import threading
import time


def until(predicate, timeout=5):
  deadline = time.monotonic() + timeout
  while time.monotonic() < deadline:
    value = predicate()
    if value:
      return value
    threading.Event().wait(.005)
  raise RuntimeError('owned EINTR receipt gate exceeded')


class Peer:
  def __init__(self):
    self.received = threading.Event()
    self.release = threading.Event()
    self.rows = []
    self.errors = []
    self.closed = False
    owner = self
    class Handler(BaseHTTPRequestHandler):
      def log_message(self, *args):
        pass
      def do_POST(self):
        body = self.rfile.read(int(self.headers['Content-Length']))
        owner.rows.append({'path': self.path, 'body': body.hex(), 'time': time.monotonic()})
        owner.received.set()
        if not owner.release.wait(20):
          owner.errors.append('fixture response gate exceeded')
          return
        response = b'{"ok":true,"token":"owned-session"}'
        try:
          self.send_response(200)
          self.send_header('Content-Length', str(len(response)))
          self.end_headers()
          self.wfile.write(response)
        except OSError as error:
          owner.errors.append(str(error))
    self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
    self.thread.start()
    self.base = f'http://127.0.0.1:{self.server.server_port}'
  def close(self):
    if self.closed:
      return
    self.closed = True
    self.release.set()
    self.server.shutdown()
    self.server.server_close()
    self.thread.join()


def child_pid(tracer, expected_executable=None):
  children = Path(f'/proc/{tracer.pid}/task/{tracer.pid}/children')
  def attempt():
    if tracer.poll() is not None:
      raise RuntimeError(f'owned EINTR tracer exited before child discovery: {tracer.returncode}')
    try:
      value = children.read_text().strip()
    except (FileNotFoundError, ProcessLookupError):
      return None
    if value.isascii() and value.isdigit():
      pid = int(value)
      if pid > 0 and pid != tracer.pid:
        if expected_executable is not None:
          try:
            actual = os.readlink(f'/proc/{pid}/exe')
          except (FileNotFoundError, ProcessLookupError):
            return None
          if actual != expected_executable:
            return None
        return pid
    return None
  return until(attempt)


def blocked(pid):
  path = Path(f'/proc/{pid}/syscall')
  if not path.exists():
    return None
  value = path.read_text().strip()
  calls = {0, 7, 45, 63, 73, 128, 207, 219, 271}
  return value if value.split()[0].isdigit() and int(value.split()[0]) in calls else None


def interrupt(pid, trace, receipts):
  syscall = until(lambda: blocked(pid))
  before = len(trace.read_text())
  os.kill(pid, signal.SIGSTOP)
  until(lambda: (
    'SIGSTOP' in trace.read_text()[before:] and '\nState:\t' in Path(f'/proc/{pid}/status').read_text()
    and Path(f'/proc/{pid}/status').read_text().split('State:\t')[1].startswith(('T', 't'))
  ))
  os.kill(pid, signal.SIGCONT)
  until(lambda: 'SIGCONT' in trace.read_text()[before:])
  receipts.append({'pid': pid, 'blocked_syscall': syscall, 'resumed_at': time.monotonic(), 'trace_offset': before})


def launch(command, root, request, env):
  root.mkdir(parents=True)
  trace = root / 'syscalls.log'
  stderr = root / 'stderr.log'
  # This applies only to failed fixture cleanup; normal STOP/CONT receipts and
  # upload deadlines are unchanged. Never leave a tracee when its tracer dies.
  invocation = ['strace', '--kill-on-exit', '-qq', '-f', '-ttt', '-o', str(trace), '-e', 'trace=network,poll,ppoll,restart_syscall,signal'] + command
  with stderr.open('w') as errors:
    process = subprocess.Popen(invocation, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, text=True, env=env)
  pid = None
  try:
    process.stdin.write(json.dumps(request) + '\n')
    process.stdin.close()
    process.stdin = None
    expected = str(Path(command[0]).resolve())
    # A unique startup child can exit before exec. Retry selection and identity
    # together so a vanished transient PID never pins subsequent attempts.
    pid = child_pid(process, expected)
    identity = {'pid': pid, 'executable': expected}
    return process, pid, trace, invocation, identity
  except BaseException as error:
    if process.stdin is not None:
      try:
        process.stdin.close()
      except BrokenPipeError:
        pass
      process.stdin = None
    stop_child(process)
    if isinstance(error, Exception):
      with stderr.open('rb') as source:
        source.seek(0, os.SEEK_END)
        source.seek(max(0, source.tell() - 4096))
        detail = source.read(4096).decode(errors='replace')
      raise RuntimeError(f'owned EINTR launch failed: {error}; tracer exit {process.returncode}; stderr: {detail}') from error
    raise


def stop_child(process):
  if process.poll() is None:
    process.kill()
  process.communicate(timeout=5)

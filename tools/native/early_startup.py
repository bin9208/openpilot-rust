"""Original startup display and standalone recovery, before native compilation."""
from contextlib import suppress
import os
from pathlib import Path
import socket
import subprocess
import time


def interpreter():
  for name in ('/usr/local/venv/bin/python3', '/usr/local/venv/bin/python', '/usr/bin/python3'):
    if Path(name).is_file():
      return name
  raise FileNotFoundError('startup Python interpreter unavailable')


def listening(port=6999):
  try:
    with socket.create_connection(('127.0.0.1', port), timeout=0.1):
      return True
  except OSError:
    return False


class Startup:
  def __init__(self, source, *, log=Path('/tmp/launch_log'), port=6999):
    self.source = Path(source)
    self.port = port
    self.log = Path(log)
    self.output = None
    self.spinner = None
    self.spinner_native = False
    self.recovery = None
    self.started = time.monotonic()
    self.phase = 'Starting build'
    self.last_update = 0.0
    self.last_message = ''

  def command(self, binary, module):
    native = self.source / 'rust/bin' / binary
    if native.is_file() and os.access(native, os.X_OK):
      return [str(native)], True
    return [interpreter(), '-B', '-m', module], False

  def environment(self):
    env = dict(os.environ, PYTHONPATH=str(self.source), PYTHONUNBUFFERED='1')
    env.pop('LD_PRELOAD', None)
    env['LD_LIBRARY_PATH'] = str(self.source / 'rust/bin/lib')
    for key in ('OPENPILOT_ROOT', 'OPENPILOT_BASEDIR', 'BASEDIR'):
      env[key] = str(self.source)
    return env

  def start(self):
    self.output = self.log.open('ab', buffering=0)
    if not listening(self.port):
      argv, native = self.command('openpilot-carrot-recovery', 'openpilot.selfdrive.carrot.recovery.server')
      for attempt in range(2 if native else 1):
        if attempt:
          argv = [interpreter(), '-B', '-m', 'openpilot.selfdrive.carrot.recovery.server']
        self.recovery = subprocess.Popen([*argv, '--port', str(self.port)], cwd=self.source,
            env=self.environment(), stdout=self.output, stderr=self.output, start_new_session=True)
        until = time.monotonic() + 5
        while not listening(self.port):
          if self.recovery.poll() is not None or time.monotonic() >= until:
            if self.recovery.poll() is None:
              self.recovery.terminate()
              self.recovery.wait(timeout=2)
            break
          time.sleep(0.05)
        else:
          break
      else:
        raise RuntimeError(f'recovery port {self.port} did not start; see {self.log}')
    argv, native = self.command('openpilot-spinner', 'openpilot.system.ui.spinner')
    if native:
      argv += ['--source-root', str(self.source)]
    self.spinner = subprocess.Popen(argv, cwd=self.source, env=self.environment(), stdin=subprocess.PIPE,
        stdout=self.output, stderr=self.output, start_new_session=True)
    self.spinner_native = native
    self.update('Preparing build')
    return self

  def update(self, phase=None, *, detail=None):
    if phase:
      self.phase = phase
    now = time.monotonic()
    if phase is None and now - self.last_update < 1:
      return
    self.last_update = now
    elapsed = int(now - self.started)
    message = f'{self.phase} · {elapsed // 60:02}:{elapsed % 60:02}'
    if detail:
      message += ' · ' + ' '.join(detail.split())[-180:]
    self.last_message = message
    if self.spinner is not None and self.spinner.poll() is not None:
      if not self.spinner_native:
        raise RuntimeError(f'progress display exited; see {self.log}; recovery port {self.port} is available')
      self.close_spinner()
      self.spinner = subprocess.Popen([interpreter(), '-B', '-m', 'openpilot.system.ui.spinner'],
          cwd=self.source, env=self.environment(), stdin=subprocess.PIPE,
          stdout=self.output, stderr=self.output, start_new_session=True)
      self.spinner_native = False
    if self.spinner is not None and self.spinner.poll() is None:
      with suppress(OSError, ValueError):
        self.spinner.stdin.write((message + '\n').encode())
        self.spinner.stdin.flush()

  def close_spinner(self):
    child, self.spinner = self.spinner, None
    if child is not None:
      if child.poll() is None:
        child.terminate()
      with suppress(subprocess.TimeoutExpired):
        child.wait(timeout=2)
      if child.poll() is None:
        child.kill()
        child.wait()
      if child.stdin is not None:
        child.stdin.close()

  def failure(self, error):
    self.close_spinner()
    message = f'Build failed\n\n{error}\n\nRecovery: port {self.port}\nLog: {self.log}'
    print(message, flush=True)
    argv, native = self.command('openpilot-text-window', 'openpilot.system.ui.text')
    if native:
      argv += ['--source-root', str(self.source), '--text', message]
    else:
      argv += [message]
    # Keep recovery available while the original error screen waits for the
    # user's action. Reboot/normal signals still end the startup process.
    for attempt in range(2 if native else 1):
      if attempt:
        argv = [interpreter(), '-B', '-m', 'openpilot.system.ui.text', message]
      child = subprocess.Popen(argv, cwd=self.source, env=self.environment(), stdout=self.output,
                               stderr=self.output, start_new_session=True)
      try:
        while child.poll() is None:
          time.sleep(0.2)
        if child.poll() == 0:
          break
      finally:
        if child.poll() is None:
          child.terminate()

  def handoff(self):
    self.update('Build complete · starting openpilot')
    self.close_spinner()
    if self.output is not None:
      self.output.close()
      self.output = None
    # The standalone original recovery intentionally survives the handoff.
    # The launcher reuses its already listening port rather than spawning a
    # second server. It remains usable when the manager/UI fails to start.

"""Source-oracle listener boundary for terminal EOF preceding waitpid status.

Original source bytes and method bodies remain intact. A missing exit status
is resolved from that reader's actual child, never from the expected result.
"""
import json
from pathlib import Path
import threading


def install_exit_status_barrier(module, output):
  local = threading.local()
  read_loop, send_json = module.RecoveryPty._read_loop, module.WsConn.send_json
  output = Path(output)

  def reader(terminal):
    with terminal.lock:
      local.process = terminal.proc
    try:
      return read_loop(terminal)
    finally:
      del local.process

  def send(connection, value):
    if value.get('type') == 'pty_exit' and value.get('exit_code') is None:
      process = getattr(local, 'process', None)
      if process is None:
        raise RuntimeError('PTY EOF has no owned reader child')
      # The original reader has released the session lock and dropped proc.
      # Timeout or an unknown child fails the comparison, without retry.
      code = process.wait(timeout=2)
      if type(code) is not int:
        raise RuntimeError('PTY child did not yield an actual exit status')
      with (output/('pty-exit-'+str(process.pid)+'.json')).open('x') as stream:
        json.dump(dict(pid=process.pid, observed_exit_code=None, reaped_exit_code=code,
                       boundary='source listener EOF/waitpid synchronization'), stream)
      value = dict(value, exit_code=code)
    return send_json(connection, value)

  module.RecoveryPty._read_loop = reader
  module.WsConn.send_json = send

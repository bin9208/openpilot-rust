from __future__ import annotations

import hashlib
import http.client
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
BINDING = ROOT / '.analysis/scratch/2026-10-01-rust-ui-application/worktree/.omo/evidence/plannerd-review-197/ci-cython-params-v2/params_pyx.cpython-312-x86_64-linux-gnu.so'


def until(predicate, timeout: float = 6):
  deadline = time.monotonic()+timeout
  while time.monotonic() < deadline:
    result = predicate()
    if result:
      return result
    time.sleep(0.01)
  raise TimeoutError('owned heartbeat observation deadline')


class Driver:
  def __init__(self, side: str, binary: Path | None, root: Path, peer, has_params: bool = True, params: tuple[tuple[str, bytes], ...] = (), times: tuple[float, ...] = (1700000000.9, 1700000001.25), ips: tuple[str, ...] = ('127.0.0.2', '127.0.0.3')):
    root.mkdir(parents=True)
    params_root = root / 'params'
    (params_root / 'd').mkdir(parents=True)
    for name, data in params:
      (params_root / 'd' / name).write_bytes(data)
    config = dict(params_root=str(params_root), has_params=has_params, binding=str(BINDING), peer=f'{peer.address[0]}:{peer.address[1]}', tls=bool(peer.tls), endpoint=peer.endpoint, times=times, ips=ips)
    command = [str(binary.resolve())] if side == 'native' else [sys.executable, '-P', str(ROOT / 'rust/tools/carrot_server_heartbeat_source.py')]
    env = dict(os.environ, OPENPILOT_PREFIX='d', NO_PROXY='127.0.0.1,localhost', PYTHONPATH=str(ROOT / 'rust/tools')+':'+str(ROOT)+':'+os.environ.get('PYTHONPATH', ''))
    self.root = root
    self.output = []
    self.replies = queue.Queue()
    self.stderr = (root / 'stderr.log').open('w')
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True, env=env, cwd=ROOT)
    self.thread = threading.Thread(target=self.read, daemon=True)
    self.thread.start()
    (root / 'invocation.json').write_text(json.dumps(dict(command=command, cwd=str(ROOT), config=config, PYTHONPATH=env['PYTHONPATH'], binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest() if binary else None), indent=2)+'\n')
    self.process.stdin.write(json.dumps(config)+'\n')
    self.process.stdin.flush()
    self.port = self.replies.get(timeout=10)['port']
    self.commands = []

  def read(self) -> None:
    for line in self.process.stdout:
      self.output.append(line)
      self.replies.put(json.loads(line))

  def command(self, operation: str, **kwargs):
    command = dict(operation=operation, **kwargs)
    self.commands.append(command)
    self.process.stdin.write(json.dumps(command)+'\n')
    self.process.stdin.flush()
    return self.replies.get(timeout=12)['result']

  def http(self, method: str = 'GET'):
    connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=3)
    connection.request(method, '/api/heartbeat_status')
    response = connection.getresponse()
    result = dict(status=response.status, headers=list(response.getheaders()), body=response.read().hex())
    connection.close()
    return result

  def close(self) -> None:
    self.process.stdin.close()
    try:
      self.process.wait(10)
    except subprocess.TimeoutExpired:
      self.process.kill()
      self.process.wait()
      raise
    finally:
      self.thread.join(1)
      self.stderr.close()
      (self.root / 'stdout.log').write_text(''.join(self.output))
      (self.root / 'commands.json').write_text(json.dumps(self.commands, indent=2)+'\n')
      (self.root / 'completion.json').write_text(json.dumps(dict(exit_code=self.process.returncode), indent=2)+'\n')
    assert self.process.returncode == 0, (self.process.returncode, (self.root / 'stderr.log').read_text())

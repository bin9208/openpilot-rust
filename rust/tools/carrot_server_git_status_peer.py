from __future__ import annotations

import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time


WRAPPER = '''#!/usr/bin/python3
import json,os,subprocess,sys,time
args=sys.argv[1:]
record={'args':args,'pid':os.getpid(),'time':time.monotonic(),'lock_inherited':False}
for name in os.listdir('/proc/self/fd'):
 try:
  if os.readlink('/proc/self/fd/'+name)==os.environ['CARROT_REPO_LOCK_PATH']:record['lock_inherited']=True
 except FileNotFoundError:pass
fd=os.open(os.environ['OWNED_GIT_TRACE'],os.O_CREAT|os.O_WRONLY|os.O_APPEND,0o600)
os.write(fd,(json.dumps(record)+'\\n').encode());os.close(fd)
if args[0]==os.environ.get('OWNED_GIT_BLOCK'):
 child=subprocess.Popen(['/bin/sh','-c','trap "" TERM; printf R; read value < "$OWNED_GIT_GATE"'],stdout=subprocess.PIPE)
 assert child.stdout.read(1)==b'R'
 open(os.environ['OWNED_GIT_DESCENDANT'],'w').write(str(child.pid))
 notice=os.environ.get('OWNED_GIT_NOTICE')
 if notice:
  with open(notice,'wb',buffering=0) as out:out.write(b'R')
 if os.environ.get('OWNED_GIT_EXIT_LEADER')=='1':sys.exit(0)
 child.wait()
os.execv('/usr/bin/git',['git',*args])
'''


class Peer:
  def __init__(self, command: list[str], repository: Path, root: Path, env: dict[str, str], launcher: Path):
    root.mkdir(parents=True)
    self.root = root
    self.trace = root / 'git-commands.jsonl'
    self.descendant = root / 'descendant-pid'
    self.gate = root / 'gate'
    os.mkfifo(self.gate)
    wrapper = root / 'bin/git'
    wrapper.parent.mkdir()
    wrapper.write_text(WRAPPER)
    wrapper.chmod(0o700)
    self.env = dict(env, PATH=str(wrapper.parent) + ':' + env['PATH'], OWNED_GIT_TRACE=str(self.trace), OWNED_GIT_GATE=str(self.gate), OWNED_GIT_DESCENDANT=str(self.descendant))
    self.stderr = (root / 'stderr.log').open('w')
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True, env=self.env)
    self.queue: queue.Queue[str | None] = queue.Queue()
    self.reader = threading.Thread(target=self._read, daemon=True)
    self.reader.start()
    self.observations: list[dict] = []
    self.diagnostics: list[str] = []
    self.config = dict(repo=str(repository), lock=env['CARROT_REPO_LOCK_PATH'], launcher=str(launcher))
    self.process.stdin.write(json.dumps(self.config) + '\n')
    self.process.stdin.flush()
    (root / 'invocation.json').write_text(json.dumps(dict(command=command, env=self.env, stdin_configuration=self.config), indent=2) + '\n')

  def _read(self) -> None:
    for line in self.process.stdout:
      self.queue.put(line)
    self.queue.put(None)

  def request(self, operation: str, **fields):
    payload = dict(operation=operation, **fields)
    started = time.monotonic()
    self.process.stdin.write(json.dumps(payload) + '\n')
    self.process.stdin.flush()
    while True:
      line = self.queue.get(timeout=40)
      if line is None:
        raise RuntimeError(f'owned peer exited: {self.process.poll()} {self.root}')
      try:
        response = json.loads(line)
      except json.JSONDecodeError:
        self.diagnostics.append(line)
        continue
      self.observations.append(dict(request=payload, response=response, elapsed=time.monotonic() - started))
      return response

  def git_trace(self) -> list[dict]:
    return [json.loads(line) for line in self.trace.read_text().splitlines()] if self.trace.exists() else []

  def close(self) -> None:
    self.process.stdin.close()
    code = self.process.wait(timeout=5)
    self.reader.join(timeout=1)
    self.stderr.close()
    (self.root / 'observations.json').write_text(json.dumps(self.observations, indent=2) + '\n')
    (self.root / 'stdout-diagnostics.json').write_text(json.dumps(self.diagnostics, indent=2) + '\n')
    (self.root / 'exit.json').write_text(json.dumps(dict(code=code)) + '\n')
    if code != 0:
      raise RuntimeError(f'owned peer failed: {code}, {self.root}')

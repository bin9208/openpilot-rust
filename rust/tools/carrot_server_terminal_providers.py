# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned terminal providers: real bash/PTY, no workstation login/MOTD/tmux effects."""

from __future__ import annotations

import os
from pathlib import Path
import sys


def prepare(root: Path) -> dict[str, str]:
  for name in ['bin', 'repository', 'home', 'motd']:
    (root / name).mkdir(parents=True, exist_ok=True)
  (root / 'motd-cache').write_text('owned MOTD\n')
  recorder = [
    f'#!{sys.executable}',
    'import os,sys,json,fcntl,termios,struct',
    f'path={str(root / "shell.jsonl")!r}',
    'record={"pid":os.getpid(),"sid":os.getsid(0),"argv":sys.argv[1:],"stdin":os.ttyname(0),"stdout":os.ttyname(1),"stderr":os.ttyname(2),"size":list(struct.unpack("HHHH",fcntl.ioctl(0,termios.TIOCGWINSZ,b"\\0"*8))),"tmux":os.getenv("TMUX"),"term":os.getenv("TERM"),"colorterm":os.getenv("COLORTERM")}',
    'with open("/proc/self/stat") as stream: record["starttime"]=int(stream.read().rsplit(")",1)[1].split()[19])',
    'try:',
    '  fd=os.open("/dev/tty",os.O_RDWR);os.close(fd);record["controlling_tty"]=True',
    'except OSError as error: record["controlling_tty"]=False;record["ctty_errno"]=error.errno',
    'with open(path,"a") as output: output.write(json.dumps(record)+"\\n")',
    'os.execv("/bin/bash",["bash","--noprofile","--norc","-c",sys.argv[2]])',
  ]
  shell = root / 'bin/owned-shell'
  shell.write_text('\n'.join(recorder) + '\n')
  shell.chmod(0o755)
  bash = root / 'bin/bash'
  bash.write_text('#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n')
  bash.chmod(0o755)
  environment = os.environ | {
    'PATH': str(root / 'bin') + os.pathsep + os.environ['PATH'],
    'SHELL': str(shell),
    'HOME': str(root / 'home'),
    'USER': 'comma',
    'PS1': 'owned> ',
    'TMUX': 'owned-sentinel',
  }
  for name in ['TERM', 'COLORTERM', 'BASH_ENV', 'ENV']:
    environment.pop(name, None)
  return environment

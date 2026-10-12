# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import json
from pathlib import Path
import sys


def prepare(root: Path, port: int) -> None:
  proc = root / 'proc'
  (proc / 'net').mkdir(parents=True)
  for name in ['tcp', 'tcp6', 'udp', 'udp6']:
    (proc / 'net' / name).write_text('\n'.join(f'{name} row {index}' for index in range(350)))
  (proc / 'uptime').write_text('123.45 222.22\n')
  (proc / 'loadavg').write_text('0.01 0.02 0.03 1/100 42\n')
  matches = [
    ['camerad', str(root / 'camerad')], ['stream_encoderd', str(root / 'stream_encoderd')],
    ['encoderd', str(root / 'stream_encoderd')], ['webrtcd', str(root / 'webrtcd')],
    ['carrot_web', 'owned-carrot-web'],
  ]
  for index, (_, pattern) in enumerate(matches):
    pid = 42000 + index
    child = proc / str(pid)
    (child / 'fd').mkdir(parents=True)
    for descriptor in range(3):
      (child / 'fd' / str(descriptor)).touch()
    (child / 'cmdline').write_bytes(pattern.encode() + b'\0--owned\xff\0')
    (child / 'status').write_text(f'Name:\towned\nPid:\t{pid}\nPPid:\t1\nThreads:\t2\nVmRSS:\t32 kB\nVmSize:\t128 kB\nIgnored:\toutside\n')
    (child / 'stat').write_text(' '.join([str(pid), '(owned)', 'S'] + [str(value) for value in range(3, 40)]))
  journal = b'outside term\n' + b'\n'.join(f'carrot journal {index}'.encode() for index in range(270)) + b'\ncamera invalid \xff\n'
  sockets = b'outside socket\n' + b'\n'.join(f'tcp :5001 pid=42000, row {index}'.encode() for index in range(340)) + b'\n'
  for name, output in [('journalctl', journal), ('ss', sockets)]:
    program = root / 'bin' / name
    program.write_text(f'#!{sys.executable}\nimport sys\nsys.stdout.buffer.write({output!r})\n')
    program.chmod(0o755)
  (root / 'vision.log').write_bytes(b'\n'.join(f'vision line {index}'.encode() for index in range(250)) + b'\ninvalid \xff\n')
  (root / 'diagnostic-config.json').write_text(json.dumps({'proc_root': str(proc), 'port': port, 'process_matches': matches}))

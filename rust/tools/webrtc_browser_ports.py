# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3", "dnspython==2.8.0"]
# ///
# Run with the pinned source-oracle environment and owned output/IPC roots.
import os
from pathlib import Path
import socket
import struct


def ports(sdp, browser_pid, node_pid):
  root = Path(f'/proc/{browser_pid}')
  assert (root / 'stat').read_text().split(') ', 1)[1].split()[1] == str(node_pid)
  assert b'browser-profile-' in (root / 'cmdline').read_bytes()
  parents = {}
  for entry in Path('/proc').iterdir():
    if entry.name.isdecimal():
      try:
        parents[int(entry.name)] = int((entry / 'stat').read_text().split(') ', 1)[1].split()[1])
      except (FileNotFoundError, ProcessLookupError):
        continue
  owned = {browser_pid}
  while True:
    next_owned = owned | {pid for pid, parent in parents.items() if parent in owned}
    if next_owned == owned:
      break
    owned = next_owned
  inodes = set()
  for pid in owned:
    for descriptor in Path(f'/proc/{pid}/fd').iterdir():
      try:
        target = os.readlink(descriptor)
      except FileNotFoundError:
        continue
      if target.startswith('socket:['):
        inodes.add(target[8:-1])
  sockets = {}
  for row in Path(f'/proc/{browser_pid}/net/udp').read_text().splitlines()[1:]:
    fields = row.split()
    if fields[9] not in inodes:
      continue
    address, port = fields[1].split(':')
    sockets[int(port, 16)] = socket.inet_ntoa(struct.pack('<I', int(address, 16)))
  result, observations = {}, []
  for line in sdp.splitlines():
    if line.startswith('a=candidate:'):
      fields = line.split()
      if fields[2].lower() == 'udp' and fields[4].endswith('.local'):
        port = int(fields[5])
        address = sockets[port]
        if address == '0.0.0.0':
          address = '127.0.0.1'
        name = fields[4].lower()
        assert name not in result or result[name] == address
        result[name] = address
        observations.append({'hostname': name, 'port': port, 'address': address, 'browser_pid': browser_pid, 'owned_processes': sorted(owned)})
  assert result
  return result, observations

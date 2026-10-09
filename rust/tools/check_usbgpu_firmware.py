"""Exact descriptor bytes and entry points from eight pinned AMD firmware blobs."""

from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))
from tinygrad.runtime.support.am import amdev


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--firmware', required=True, type=Path)
  parser.add_argument('--evidence', required=True, type=Path)
  args = parser.parse_args()
  loads = []

  def fetch(path, name, expected):
    assert path == 'amdgpu'
    data = (args.firmware / name).read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    assert digest == expected
    loads.append({'name': name, 'size': len(data), 'sha256': digest})
    return data

  amdev.fetch_fw = fetch
  device = types.SimpleNamespace(ip_ver={1: (12, 0, 0), 3: (7, 0, 0), 15: (14, 0, 2), 16: (14, 0, 2)})
  firmware = amdev.AMFirmware(device)

  def blob(value):
    return {'size': len(value), 'sha256': hashlib.sha256(value).hexdigest()}

  def descriptor(value):
    return {'kinds': value[0], **blob(value[1])}

  expected = {
    'sos': {str(key): blob(value) for key, value in firmware.sos_fw.items()},
    'descriptors': [descriptor(value) for value in firmware.descs],
    'smu': descriptor(firmware.smu_psp_desc),
    'ucode_start': firmware.ucode_start,
  }
  process = subprocess.run([args.binary, str(args.firmware)], text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = json.loads(process.stdout)
  args.evidence.write_text(
    json.dumps({'invocation': process.args, 'loads': loads, 'source': expected, 'native': actual, 'passed': expected == actual}, indent=2) + '\n'
  )
  assert expected == actual
  print(f'PASS {len(loads)} pinned firmware blobs: exact SOS/SMU/{len(firmware.descs)} descriptors and microcode entry addresses')


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
"""Inspect prepared firmware bytes and the existing public debug signature.

This Actions fixture never opens a device, reads a private key or runs firmware.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import runpy
import struct


def digest(path):
  with path.open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def public_key(path):
  fields = path.read_text().split()
  assert fields[0] == 'ssh-rsa'
  blob = base64.b64decode(fields[1], validate=True)
  parts = []
  while blob:
    assert len(blob) >= 4
    size = int.from_bytes(blob[:4], 'big')
    assert 0 < size <= len(blob)-4
    parts.append(blob[4:4+size])
    blob = blob[4+size:]
  assert len(parts) == 3 and parts[0] == b'ssh-rsa'
  exponent, modulus = (int.from_bytes(value, 'big') for value in parts[1:])
  assert exponent == 65537 and modulus.bit_length() == 1024
  return exponent, modulus


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--provider', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  source, provider = args.source.resolve(), args.provider.resolve()
  contract = json.loads((source/'rust/native-panda-source-contract.json').read_text())
  provenance = json.loads((provider/'rust/bin/licenses/panda/provenance.json').read_text())
  assert provenance['source_unchanged'] is True
  assert len(provenance['source_inputs']) == 251
  assert provenance['compiler_sha256'] == '784012433f49b420ca5d63611e36ef1dab7764dde3bb1d164da39ea67b27d300'
  for path, expected in provenance['source_inputs'].items():
    assert digest(source/path) == expected
  for path, expected in contract['outputs'].items():
    actual = provider/path
    assert not actual.is_symlink() and digest(actual) == expected
    assert actual.stat().st_mode & 0o7777 == 0o444
  version_fn = runpy.run_path(str(source/'panda/firmware_version.py'))['firmware_source_version']
  version = 'DEV-'+version_fn(source/'panda', source/'opendbc/safety')+'-DEBUG'
  assert version == provenance['firmware_version'] == 'DEV-1de1c7b1f187-DEBUG'
  exponent, modulus = public_key(source/'panda/certs/debug.pub')
  checked = []
  for artifact in provenance['artifacts']:
    path = 'panda/board/obj/'+artifact['name']
    assert path in contract['outputs'] and contract['outputs'][path] == artifact['sha256']
    data = (provider/path).read_bytes()
    assert len(data) == artifact['size'] and version.encode() in data
    signed = artifact['name'].endswith('.signed')
    if signed:
      length = struct.unpack('<I', data[:4])[0]
      assert 8 <= length and length+128 == len(data)
      assert data[length-8:length] == b'VERS'+struct.pack('<I', 2)
      expected = b'\x00\x01'+b'\xff'*0x69+b'\x00'+hashlib.sha1(data[4:length]).digest()
      actual = pow(int.from_bytes(data[length:], 'big'), exponent, modulus).to_bytes(128, 'big')
      assert actual == expected
    checked.append(dict(path=path, bytes=len(data), sha256=artifact['sha256'],
                        firmware_version_matches=True, debug_signature_verified=signed))
  assert len(checked) == 4 and len({row['path'] for row in checked}) == 4
  args.output.write_text(json.dumps(dict(schema=1, original_source_commit=contract['original_source_commit'],
      source_input_hashes_checked=251, firmware_version=version, artifacts=checked,
      firmware_regenerated=False, private_key_read=False, firmware_executed=False,
      device_connected=False, physical_device_execution=False), indent=2)+'\n')


if __name__ == '__main__':
  main()

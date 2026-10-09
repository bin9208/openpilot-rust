"""Compare native ELF placement and AMD REL64 against the unchanged source loader."""

from __future__ import annotations
import argparse
import json
from pathlib import Path
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))
from tinygrad.runtime.support.elf import elf_loader


def fixture(wide, fixed, relocation=5):
  names = ['', '.text', '.rodata', '.shstrtab', '.symtab', '.rela.text']
  strings = b'\0'
  offsets = [0]
  for name in names[1:]:
    offsets.append(len(strings))
    strings += name.encode() + b'\0'
  symbol = struct.pack('<IBBHQQ', 0, 0, 0, 2, 8, 0) if wide else struct.pack('<IIIBBH', 0, 8, 0, 0, 0, 2)
  reloc = struct.pack('<QQq', 8, relocation, -4) if wide else struct.pack('<IIi', 8, relocation, -4)
  descriptor = bytearray(64)
  struct.pack_into('<III', descriptor, 0, 1024, 32, 24)
  struct.pack_into('<q', descriptor, 16, -16)
  content = [b'', bytes(range(32)), bytes(descriptor), strings, symbol, reloc]
  header_size, section_size = (64, 64) if wide else (52, 40)
  blob = bytearray(header_size)
  sections = []
  for index, data in enumerate(content):
    addr = (0x1000 + index * 256) if fixed and index in [1, 2] else 0
    args = [offsets[index], [0, 1, 1, 3, 2, 4][index], 0, addr, len(blob), len(data), 0, 0, 16, len(data) if index in [4, 5] else 0]
    sections.append(struct.pack('<IIQQQQIIQQ' if wide else '<IIIIIIIIII', *args))
    blob.extend(data)
  section_offset = len(blob)
  blob.extend(b''.join(sections))
  ident = b'\x7fELF' + bytes([2 if wide else 1, 1, 1]) + bytes(9)
  fields = [ident, 1, 224, 1, 0, 0, section_offset, 0, header_size, 0, 0, section_size, len(sections), 3]
  blob[:header_size] = struct.pack('<16sHHIQQQIHHHHHH' if wide else '<16sHHIIIIIHHHHHH', *fields)
  return blob


def original(blob):
  try:
    image, sections, relocations = elf_loader(blob)
    for offset, symbol, kind, addend in relocations:
      if kind != 5:
        raise RuntimeError('unknown AMD relocation')
      image[offset : offset + 8] = struct.pack('<q', symbol - offset + addend)
    return {
      'bytes': list(image),
      'sections': [
        {
          'name': section.name,
          'kind': section.header.sh_type,
          'address': section.header.sh_addr,
          'offset': section.header.sh_offset,
          'size': section.header.sh_size,
          'alignment': section.header.sh_addralign,
          'entry_size': section.header.sh_entsize,
        }
        for section in sections
      ],
      'relocations': [{'offset': offset, 'symbol': symbol, 'kind': kind, 'addend': addend} for offset, symbol, kind, addend in relocations],
    }
  except (AssertionError, ValueError, RuntimeError, IndexError):
    return {'failed': True}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--kernels', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(exist_ok=True, parents=True)
  paths = sorted(args.kernels.glob('*.elf'))
  for wide in [False, True]:
    for fixed in [False, True]:
      for relocation in [5, 9]:
        path = args.evidence / f'fixture-{wide}-{fixed}-{relocation}.elf'
        path.write_bytes(fixture(wide, fixed, relocation))
        paths.append(path)
  process = subprocess.run([args.binary], input=''.join(str(path) + '\n' for path in paths), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  rows = []
  for path, native in zip(paths, actual, strict=True):
    source = original(path.read_bytes())
    passed = 'error' in native if source.get('failed') else source == native
    rows.append({'path': str(path), 'source': source, 'native': native, 'passed': passed})
  (args.evidence / 'comparison.json').write_text(json.dumps({'invocation': [args.binary], 'results': rows}, indent=2) + '\n')
  assert all(row['passed'] for row in rows), [row['path'] for row in rows if not row['passed']]
  print(f'PASS {len(rows)} actual-model ELF and 32/64-bit placement/REL64/rejection comparisons')


if __name__ == '__main__':
  main()

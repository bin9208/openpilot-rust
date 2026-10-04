from __future__ import annotations

from collections import Counter
import struct
from typing import Literal, assert_never

CleanupKind = Literal['disabled', 'probe-none', 'sensor-init', 'sensor-poke']


def word(row: dict, field: str, offset: int = 0) -> int:
  return int.from_bytes(bytes.fromhex(row[field])[offset:offset + 4], 'little')


def calls(records: list[dict]) -> list[tuple[int, int, dict]]:
  target = -1
  result = []
  for index, row in enumerate(records):
    if row['op'] == 'target': target = row['fd']
    if row['op'] == 'ioctl': result.append((index, target, row))
  return result


def audit(records: list[dict]) -> dict[str, int]:
  owned = {name: Counter() for name in ('devices', 'sessions', 'started', 'links', 'deactivation', 'buffers', 'fences', 'fds', 'mappings')}
  released = {name: Counter() for name in owned}
  image_fds = {row['fd'] for row in records if row['op'] == 'image-import'}
  imports = set()
  for row in records:
    if row['op'] in ('open', 'vision-open'): owned['fds'][row['fd']] += 1
    if row['op'] == 'close': released['fds'][row['fd']] += 1
    if row['op'] in ('mmap', 'vision-mmap') and row['ok']: owned['mappings'][row['fd']] += 1
    if row['op'] in ('munmap', 'vision-munmap'): released['mappings'][row['fd']] += 1
  owned['fds'].update(image_fds - owned['fds'].keys())
  for _, target, row in calls(records):
    name, success = row['name'], row['ret'] == 0
    pair = (target, word(row, 'before'), word(row, 'before', 4))
    if name == 'camera:258' and success: owned['devices'][(target, word(row, 'after'), word(row, 'after', 4))] += 1
    if name == 'camera:262': released['devices'][pair] += 1
    if name == 'camera:267' and success: owned['sessions'][word(row, 'after')] += 1
    if name == 'camera:268': released['sessions'][word(row, 'before')] += 1
    if name == 'camera:259' and success: owned['started'][pair] += 1
    if name == 'camera:260': released['started'][pair] += 1
    if name == 'camera:269' and success:
      link = (word(row, 'after'), int.from_bytes(bytes.fromhex(row['after'])[-4:], 'little'))
      owned['links'][link] += 1
      owned['deactivation'][link] += 1
    if name == 'camera:270': released['links'][(word(row, 'before'), word(row, 'before', 4))] += 1
    if name == 'camera:278' and word(row, 'before') == 1:
      released['deactivation'][(word(row, 'before', 4), word(row, 'before', 16))] += 1
    if name == 'camera:274' and word(row, 'after', 88):
      owned['buffers'][word(row, 'after', 88)] += 1
      owned['fds'][word(row, 'after', 92)] += 1
    if name == 'camera:275' and success: imports.add(word(row, 'after', 80))
    if name == 'camera:276': released['buffers'][word(row, 'before')] += 1
    if name in ('sync:0', 'camera:0') and success: owned['fences'][word(row, 'after', 64)] += 1
    if name == 'sync:1': released['fences'][word(row, 'before', 64)] += 1
  owned['buffers'].update(imports)
  for category in owned:
    assert owned[category] == released[category], (category, dict(owned[category]), dict(released[category]))
  return {category: sum(values.values()) for category, values in owned.items()}


def normalize_disabled(source: list[dict], native: list[dict], kind: CleanupKind) -> tuple[list[dict], list[dict]]:
  targets = calls(native)
  selected = []
  match kind:
    case 'probe-none':
      return native, []
    case 'disabled' | 'sensor-init':
      wanted = ((502, 'camera:262'), (501, 'camera:268'))
      for target, name in wanted:
        matches = [(index, fd, row) for index, fd, row in targets if fd == target and row['name'] == name]
        assert len(matches) == 1, (target, name, matches)
        assert not any(fd == target and row['name'] == name for _, fd, row in calls(source))
        selected.extend(matches)
    case 'sensor-poke':
      imports = {word(row, 'after', 80) for _, _, row in targets if row['name'] == 'camera:275' and row['ret'] == 0}
      for index, target, row in targets:
        name = row['name']
        remove = (name == 'sync:1'
                  or name in ('camera:260', 'camera:262') and target in (503, 504, 506)
                  or name == 'camera:278' and word(row, 'before') == 1
                  or name == 'camera:270'
                  or name == 'camera:276' and word(row, 'before') in imports)
        if remove: selected.append((index, target, row))
      for target, name in ((501, 'camera:272'), (504, 'camera:264')):
        matches = [(index, fd, row) for index, fd, row in targets if fd == target and row['name'] == name]
        source_count = sum(fd == target and row['name'] == name for _, fd, row in calls(source))
        assert len(matches) == source_count + (1 if matches else 0)
        if matches:
          prior = [row for _, fd, row in calls(source) if fd == target and row['name'] == name]
          assert matches[-1][2] == prior[-1]
          selected.append(matches[-1])
    case unreachable:
      assert_never(unreachable)
  removed = set()
  receipt = []
  for index, target, row in selected:
    assert native[index - 1] == dict(op='target', fd=target)
    length = {'sync:1':68, 'camera:260':8, 'camera:262':8, 'camera:264':32, 'camera:268':8,
              'camera:270':8, 'camera:272':24, 'camera:276':8, 'camera:278':24}[row['name']]
    payload = bytes.fromhex(row['before'])
    envelope = struct.pack('<IIIIQ', int(row['name'].split(':')[1]), length, int(row['name'].startswith('camera:')), 0, 1).hex()
    assert len(payload) == length and row['after'] == row['before']
    assert row['outer_before'] == row['outer_after'] == envelope
    assert row['ret'] == row['errno'] == 0 and row['nested'] == ''
    if row['name'] == 'sync:1': assert payload[:64] == bytes(64)
    if row['name'] in ('camera:268', 'camera:276'): assert payload[4:] == bytes(4)
    if row['name'] == 'camera:278':
      activate, = [other for _, fd, other in calls(source) if fd == target and other['name'] == row['name'] and word(other, 'before') == 0]
      assert payload == b'\x01\x00\x00\x00' + bytes.fromhex(activate['before'])[4:]
    removed.update((index - 1, index))
    receipt.append(dict(fd=target, name=row['name'], payload=row['before']))
  return [row for index, row in enumerate(native) if index not in removed], receipt

import random

LENGTHS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64]
ADDRESSES = [0, 1, 0x7ff, 0x800, 0x1fffffff, 0x20000000, 0xffffffff]
MAX_U64 = 2**64 - 1


def frame(address, source, size, rng):
  return {'address': address, 'src': source, 'data': [rng.randrange(256) for _ in range(size)]}


def packing():
  rng = random.Random(175)
  result = []
  for offset in (0, 4, 252, 253, 255, 256, 2**32 - 4, 2**32 - 1):
    for size in LENGTHS:
      frames = [frame(ADDRESSES[source % len(ADDRESSES)], source, size, rng) for source in range(256)]
      result.append({'op': 'pack', 'offset': offset, 'frames': frames})
  for count in (0, 1, 3, 31, 32, 33, 64, 255, 1024):
    for size in (0, 8, 12, 64):
      result.append({'op': 'pack', 'offset': 0,
                     'frames': [frame(index, index % 4, size, rng) for index in range(count)]})
  for _ in range(500):
    offset = rng.choice((0, 4, 8, 252))
    result.append({'op': 'pack', 'offset': offset, 'frames': [frame(rng.randrange(2**32), rng.randrange(256), rng.choice(LENGTHS), rng)
                                                             for _ in range(rng.randrange(80))]})
  return result


def seeds():
  rng = random.Random(175)
  return [{'op': 'pack', 'offset': 0, 'frames': [frame(ADDRESSES[index % len(ADDRESSES)], 0, size, rng)]}
          for index, size in enumerate(LENGTHS)]


def decoding(encoded):
  result = []
  packets = [entry['chunks'][0] for entry in encoded]
  for packet in packets:
    for bus in range(8):
      for flags in range(4):
        raw = packet.copy()
        raw[0] = (raw[0] & 0xf1) | (bus << 1)
        raw[1] = (raw[1] & 0xfc) | flags
        raw[5] = 0
        checksum = 0
        for byte in raw:
          checksum ^= byte
        raw[5] = checksum
        for offset in (0, 4, 2**32 - 1):
          split = (bus * 4 + flags) % (len(raw) + 1)
          result.append({'op': 'decode', 'offset': offset, 'chunks': [[], raw[:split], [], raw[split:]]})
  stream = [byte for packet in packets for byte in packet]
  for size in (1, 2, 3, 4, 5, 6, 7, 8, 16, 63, 64, 65, 70, 255, 256, 257, 16384):
    result.append({'op': 'decode', 'offset': 4, 'chunks': [stream[index:index + size] for index in range(0, len(stream), size)]})
  for checksum in range(256):
    broken = packets[8].copy()
    broken[5] = checksum
    result.append({'op': 'decode', 'offset': 0, 'chunks': [packets[0] + broken + packets[-1], packets[3]]})
  for split in range(len(packets[-1]) + 1):
    result.append({'op': 'decode', 'offset': 8, 'chunks': [packets[-1][:split], packets[-1][split:]]})
  rng = random.Random(517)
  for _ in range(500):
    selected = [byte for packet in rng.choices(packets, k=rng.randrange(1, 25)) for byte in packet]
    if rng.randrange(2):
      selected[rng.randrange(len(selected))] ^= 1 << rng.randrange(8)
    chunks = []
    while selected:
      size = rng.randrange(1, 128)
      chunks.append(selected[:size])
      selected = selected[size:]
    result.append({'op': 'decode', 'offset': rng.choice((0, 4, 8)), 'chunks': chunks})
  return result


def alerts():
  result = []
  for start in (0, 1, MAX_U64 - 1000):
    for count in (0, 1, 2, 3, MAX_U64):
      for terminal in (False, True):
        operations = [{'op': 'onroad', 'now': str(start), 'value': True}]
        for elapsed in (4999, 5000, 5001, 5999, 6000, 10000, 10001, 15000, 15001):
          now = str((start + elapsed) & MAX_U64)
          operations += [{'op': 'observe', 'now': now, 'count': str(count), 'terminal': terminal}, {'op': 'ready', 'now': now}]
        operations += [{'op': 'mark', 'now': now}, {'op': 'observe', 'now': now, 'count': '3', 'terminal': True},
                       {'op': 'ready', 'now': now}, {'op': 'onroad', 'now': now, 'value': False}, {'op': 'ready', 'now': now}]
        result.append({'op': 'alerts', 'operations': operations})
  rng = random.Random(751)
  for _ in range(100):
    now, operations = rng.choice((0, MAX_U64 - 100)), []
    for _ in range(300):
      now = (now + rng.choice((0, 1, 999, 1000, 4999, 5000, 9999, 10000, 10001))) & MAX_U64
      op = rng.choices(('onroad', 'observe', 'ready', 'mark'), (3, 12, 8, 1))[0]
      value = {'op': op, 'now': str(now)}
      if op == 'onroad':
        value['value'] = rng.randrange(4) != 0
      elif op == 'observe':
        value |= {'count': str(rng.choice((0, 1, 2, 3, MAX_U64))), 'terminal': rng.randrange(4) == 0}
      operations.append(value)
    result.append({'op': 'alerts', 'operations': operations})
  return result

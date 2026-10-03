import random


def device(serial=b'panda-first', **fields):
  return {'vendor': 0x3801, 'product': 0xddcc, 'serial': list(serial), **fields}


def operation(name, steps=(), length=8, **fields):
  return {'op': name, 'request': 0xdc, 'value': 65535, 'index': 32768, 'endpoint': 0x81 if name == 'bulk_read' else 3,
          'timeout': 5, 'length': 0 if name == 'control_write' else length, 'steps': list(steps), **fields}


def cases():
  rows = []

  def add(label, operations=(), **fields):
    rows.append({'label': label, 'devices': [device()], 'operations': list(operations), **fields})

  add('normal')
  add('no-devices', devices=[])
  add('irrelevant-devices', devices=[device(vendor=0), device(product=0)])
  for code in (-99, -12, -11, -10, -9, -8, -7, -6, -5, -4, -3, -2, -1, 1):
    for field in ('init', 'config', 'claim'):
      add(f'{field}-{code}', **{field: code})
    if code < 0:
      add(f'list-{code}', list_error=code)
      add(f'open-{code}', devices=[device(open=code)])
      add(f'serial-{code}', devices=[device(serial_error=code)])
  for active in (-12, -1, 0, 1, 2):
    add(f'kernel-driver-{active}', active=active)
  for serial in (b'', b'panda-first', b'panda-second', b'missing', b'a\0b', bytes(range(26)), b'a' * 26, b'a' * 27):
    for first in (b'panda-first', b'a\0b', bytes(range(26)), b'a' * 30):
      add(f'selection-{serial.hex()}-{first.hex()}', serial=list(serial),
          devices=[device(vendor=0), device(first), device(b'panda-second')])
  add('first-open-failure-aborts-scan', devices=[device(open=-3), device(b'panda-second')], serial=list(b'panda-second'))
  add('first-serial-failure-aborts-scan', devices=[device(serial_error=-9), device(b'panda-second')], serial=list(b'panda-second'))

  for name in ('control_write', 'control_read', 'bulk_write', 'bulk_read'):
    for code in (0, -99, -12, -11, -10, -9, -8, -7, -6, -5, -4, -3, -2, -1):
      for partial in (0, 1, 7, 8):
        for repeats in (1, 2, 5):
          step = {'ret': code, 'transferred': partial, 'data': [] if name == 'control_write' else [1, 2]}
          terminal = code == -4 or (name.startswith('bulk') and code in (0, -7)) or code == 0
          steps = [step] if terminal else [step] * repeats + [{'ret': 0 if name.startswith('bulk') else 3, 'transferred': 8,
                                                               'data': [] if name == 'control_write' else [3, 4, 5]}]
          next_steps = [] if code == -4 else [{'ret': 0, 'transferred': 8}]
          add(f'{name}-{code}-{partial}-{repeats}', [operation(name, steps), operation('bulk_read', next_steps)])
    add(f'explicit-disconnect-{name}', [operation('disconnect'), operation(name)])
    for length in (0, 1, 6, 26, 255, 256, 325, 512, 2048, 16384, 65535):
      if name == 'control_write' and length:
        continue
      transferred = 0 if name == 'control_write' else length
      payload = [index & 255 for index in range(transferred)]
      add(f'length-{name}-{length}', [operation(name, [{'ret': 0 if name.startswith('bulk') else transferred,
                                                     'transferred': transferred, 'data': payload if name.endswith('read') else []}],
                                             length=length, data=payload if name == 'bulk_write' else [], timeout=0xffffffff)])
  rng = random.Random(175)
  for sequence in range(100):
    operations = []
    for _ in range(25):
      name = rng.choice(('control_read', 'control_write', 'bulk_read', 'bulk_write'))
      length = 0 if name == 'control_write' else rng.randrange(65)
      steps = [{'ret': rng.choice((-1, -2, -8, -9, -99)), 'transferred': rng.randrange(length + 1),
                'data': [rng.randrange(256) for _ in range(rng.randrange(length + 1))]} for _ in range(rng.randrange(5))]
      count = rng.randrange(length + 1)
      steps.append({'ret': rng.choice((0, -7)) if name.startswith('bulk') else count,
                    'transferred': count, 'data': [rng.randrange(256) for _ in range(count)]})
      operations.append(operation(name, steps, length=length, request=rng.randrange(256), value=rng.randrange(65536),
                                  index=rng.randrange(65536), timeout=rng.randrange(501)))
    add(f'seeded-{sequence}', operations)
  add('list-initial', mode='list', repeats=3)
  for config in ([], [device()], [device(vendor=0), device(b'second'), device(bytes(range(26)))],
                 [device(open=-3), device()], [device(), device(open=-3), device()],
                 [device(serial_error=-9), device()], [device(), device(serial_error=-9), device()]):
    add(f'list-devices-{len(rows)}', mode='list', repeats=2, devices=config)
  for code in (-1, -3, -99):
    add(f'list-error-{code}', mode='list', repeats=2, list_error=code)
  add('list-recovers-after-errors', mode='list', repeats=2)
  for threads in (2, 4, 8):
    add(f'concurrent-{threads}', mode='concurrent', threads=threads, transfers=16)
  return rows

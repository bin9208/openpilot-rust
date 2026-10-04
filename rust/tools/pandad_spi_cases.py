from functools import reduce


def checksum(data):
  return reduce(int.__xor__, data, 0xab)


def transfer(tx, rx=(), **extra):
  return {'kind': 'transfer', 'tx': list(tx), 'rx': list(rx), 'length': len(tx), **extra}


def transaction(endpoint, payload, maximum, reply):
  payload, reply = list(payload), list(reply)
  header = [0x5a, endpoint, len(payload) & 255, len(payload) >> 8, maximum & 255, maximum >> 8]
  response = [0x85, len(reply) & 255, len(reply) >> 8]
  return [transfer(header + [checksum(header)]), transfer([0x11], [0x79]),
          transfer(payload + [checksum(payload)]), transfer([0x13] * 3, response),
          {'kind': 'transfer', 'length': len(reply) + 1, 'rx': reply + [checksum(response + reply)]}]


def setup():
  return [{'kind': 'mode'}, {'kind': 'speed'}, {'kind': 'bits'}] + transaction(0, [0xc3, 0, 0, 0, 0, 12, 0], 12, range(12))


def scenarios():
  rows = [{'name': 'constructor', 'steps': setup(), 'operations': []},
          {'name': 'list', 'steps': setup(), 'operations': [], 'list': True},
          {'name': 'serial-mismatch', 'steps': setup(), 'serial': 'not-the-fixture', 'operations': []},
          {'name': 'missing-device', 'stat': {'result': -1, 'errno': 2}, 'steps': [], 'operations': []},
          {'name': 'open-denied', 'open': {'result': -1, 'errno': 13}, 'steps': [], 'operations': []}]
  for size in (0, 1, 12, 64, 1984):
    response = [(i * 17 + size) & 255 for i in range(size)]
    payload = [0xa1, 0x34, 0x12, 0x78, 0x56, size & 255, size >> 8]
    rows.append({'name': f'control-read-{size}', 'steps': setup() + transaction(0, payload, size, response),
                 'operations': [{'kind': 'control_read', 'request': 0xa1, 'param1': 0x1234, 'param2': 0x5678, 'length': size, 'timeout': 100}]})
  return rows


def oversized_response():
  return {'name': 'source-response-exceeds-caller', 'steps': setup() + transaction(0, [0xa1, 0, 0, 0, 0, 1, 0], 1, [0x31, 0x32]),
          'operations': [{'kind': 'control_read', 'request': 0xa1, 'length': 1, 'timeout': 100}]}


def recovery(restart=False):
  good = transfer([0x14] * 1024, [0x1f])
  if restart:
    return [good, transfer([0x14] * 1024, [], result=-1, errno=5), good, good, good]
  return [good, good, good]


def failure(endpoint, payload, maximum, reply, phase, timeout=1, tick_ns=1000000, restart=False):
  calls = transaction(endpoint, payload, maximum, reply)
  if phase == 'header_io':
    calls = calls[:1]
    calls[-1].update(result=-1, errno=5)
  elif phase in ('hack_nack', 'hack_io', 'hack_timeout'):
    calls = calls[:2]
    if phase == 'hack_nack': calls[-1]['rx'] = [0x1f]
    elif phase == 'hack_io': calls[-1].update(result=-1, errno=5)
    else:
      limit = max(20, min(500, timeout or 500))
      # Cross the limit by at least one nanosecond, independent of the fractional
      # millisecond epoch retained from earlier scenarios. The source compares
      # floating millisecond differences, so exact-equality poll counts vary.
      calls = calls[:1] + [transfer([0x11], [], elapsed_ns=1) for _ in range(limit * 1000000 // (tick_ns + 1) + 1)]
  elif phase == 'data_io':
    calls = calls[:3]
    calls[-1].update(result=-1, errno=5)
  elif phase in ('dack_nack', 'dack_io', 'dack_timeout'):
    calls = calls[:4]
    if phase == 'dack_nack': calls[-1]['rx'] = [0x1f, 0, 0]
    elif phase == 'dack_io': calls[-1].update(result=-1, errno=5)
    else:
      limit = max(20, min(500, timeout or 500))
      calls = calls[:3] + [transfer([0x13] * 3, [], elapsed_ns=1) for _ in range(limit * 1000000 // (tick_ns + 1) + 1)]
  elif phase == 'rx_io': calls[-1].update(result=-1, errno=5)
  elif phase == 'rx_checksum': calls[-1]['rx'][-1] ^= 1
  elif phase == 'rx_length':
    calls = calls[:4]
    calls[-1]['rx'] = [0x85, 0, 8]
  else: raise ValueError(phase)
  return calls + recovery(restart)


def full_scenarios():
  rows = scenarios()
  for index, kind in enumerate(('mode', 'speed', 'bits')):
    for as_list in (False, True):
      steps = setup()[:index + 1]
      steps[-1].update(result=-1, errno=5)
      rows.append({'name': f'setup-{kind}-failure-list-{as_list}', 'steps': steps, 'operations': [], 'list': as_list})
    steps = setup()
    steps.insert(index, {'kind': kind, 'result': -1, 'errno': 4})
    rows.append({'name': f'setup-{kind}-interrupted', 'steps': steps, 'operations': []})
  for length in (0, 1, 11):
    rows.append({'name': f'uid-short-{length}', 'steps': setup()[:3] + transaction(0, [0xc3, 0, 0, 0, 0, 12, 0], 12, list(range(length))), 'operations': []})
  for phase_index in range(5):
    steps = transaction(0, [0xa1, 0, 0, 0, 0, 1, 0], 1, [0x31])
    interrupted = dict(steps[phase_index], result=-1, errno=4)
    steps.insert(phase_index, interrupted)
    rows.append({'name': f'transfer-phase-{phase_index}-interrupted', 'steps': setup() + steps,
                 'operations': [{'kind': 'control_read', 'request': 0xa1, 'length': 1, 'timeout': 1}]})
  for length in (0, 1, 7, 2044):
    rows.append({'name': f'control-write-response-{length}', 'steps': setup() + transaction(0, [0xa1, 0, 0, 0, 0, 0, 0], 0, [i & 255 for i in range(length)]),
                 'operations': [{'kind': 'control_write', 'request': 0xa1, 'length': 0, 'timeout': 1}]})
  for timeout, failures in ((0, 8), (0xffffffff, 7)):
    payload, reply = [], [0x31]
    steps = setup() + failure(0x81, payload, 1, reply, 'hack_timeout', timeout=timeout, tick_ns=25000000) * failures
    if timeout == 0: steps += transaction(0x81, payload, 1, reply)
    rows.append({'name': f'timeout-policy-{timeout}', 'tick_ns': 25000000, 'steps': steps,
                 'operations': [{'kind': 'bulk_read', 'endpoint': 0x81, 'length': 1, 'timeout': timeout}]})
  phases = ('header_io', 'hack_nack', 'hack_io', 'hack_timeout', 'data_io', 'dack_nack', 'dack_io', 'dack_timeout', 'rx_io', 'rx_checksum', 'rx_length')
  for endpoint in (0, 3, 0x81):
    for phase in phases:
      for restart in (False, True):
        if endpoint == 0:
          payload, maximum, reply = [0xa1, 0, 0, 0, 0, 1, 0], 1, [0x31]
          op = {'kind': 'control_read', 'request': 0xa1, 'length': 1, 'timeout': 1}
        elif endpoint == 3:
          payload, maximum, reply = [0x31, 0x32], 0, []
          op = {'kind': 'bulk_write', 'endpoint': endpoint, 'length': 2, 'data': payload, 'timeout': 1}
        else:
          payload, maximum, reply = [], 2, [0x31, 0x32]
          op = {'kind': 'bulk_read', 'endpoint': endpoint, 'length': 2, 'timeout': 1}
        rows.append({'name': f'{endpoint}-{phase}-restart-{restart}', 'tick_ns': 1000000,
                     'steps': setup() + failure(endpoint, payload, maximum, reply, phase, restart=restart) + transaction(endpoint, payload, maximum, reply),
                     'operations': [op]})
  for count in (3, 4, 5, 22, 201):
    payload = [0x31, 0x32]
    failed = failure(3, payload, 0, [], 'hack_nack')
    rows.append({'name': f'nack-backoff-{count}', 'tick_ns': 1000000,
                 'steps': setup() + failed * count + transaction(3, payload, 0, []),
                 'operations': [{'kind': 'bulk_write', 'endpoint': 3, 'length': 2, 'data': payload, 'timeout': 1}]})
  for endpoint in (0, 3, 0x81):
    payload = [0xa1, 0, 0, 0, 0, 1, 0] if endpoint == 0 else []
    if endpoint == 3: payload = [0x31]
    op = {'kind': 'control_read' if endpoint == 0 else 'bulk_write' if endpoint == 3 else 'bulk_read',
          'request': 0xa1, 'endpoint': endpoint, 'length': 1, 'data': [0x31], 'timeout': 1}
    maximum, reply = (0, []) if endpoint == 3 else (1, [0x31])
    rows.append({'name': f'terminal-timeout-{endpoint}', 'tick_ns': 1000000,
                 'steps': setup() + failure(endpoint, payload, maximum, reply, 'hack_timeout') * 7, 'operations': [op]})
  for size in (0, 1, 1983, 1984, 1985, 3968, 65535):
    for write in (False, True):
      data = [(i * 13 + size) & 255 for i in range(size)]
      steps = setup()
      for i in range(0, size, 1984):
        part = data[i:i + 1984]
        steps += transaction(3 if write else 0x81, part if write else [], 0 if write else len(part), [] if write else part)
      rows.append({'name': f'bulk-{write}-{size}', 'steps': steps,
                   'operations': [{'kind': 'bulk_write' if write else 'bulk_read', 'endpoint': 3 if write else 0x81, 'length': size, 'data': data if write else [], 'timeout': 100}]})
  steps, operations = setup(), []
  for i in range(100):
    if i == 49: steps += failure(3, [i], 0, [], 'dack_nack')
    steps += transaction(3, [i], 0, [])
    operations.append({'kind': 'bulk_write', 'endpoint': 3, 'data': [i], 'length': 1, 'timeout': 1})
  rows.append({'name': 'aggregate-100', 'steps': steps, 'operations': operations, 'tick_ns': 1000000})
  return rows

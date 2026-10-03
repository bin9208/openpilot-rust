import math
from typing import TypedDict, TypeAlias, assert_never

MACS = ('00:11:22:33:44:55', '66:77:88:99:AA:BB')


class Snapshot(TypedDict):
  device_alive: bool
  started: bool
  car_alive: bool
  car_valid: bool
  can_valid: bool
  controls_alive: bool
  enabled: bool
  brake: bool
  gas: bool
  gear_drive: bool
  physical_buttons: bool
  v_ego: float


class Event(TypedDict):
  kind: int
  code: int
  value: int
  at: float


class Step(TypedDict):
  now: float
  config: str
  learning: str
  available: dict[str, str]
  errors: dict[str, str]
  snapshot: Snapshot
  reads: dict[str, list[Event]]


Scenario: TypeAlias = tuple[str, list[Step]]


def default_state() -> Snapshot:
  return {
    'device_alive': True,
    'started': True,
    'car_alive': True,
    'car_valid': True,
    'can_valid': True,
    'controls_alive': True,
    'enabled': True,
    'brake': False,
    'gas': False,
    'gear_drive': True,
    'physical_buttons': False,
    'v_ego': 0.0,
  }


def config() -> str:
  import json

  return json.dumps(
    {
      'devices': {
        mac: {
          'profile': 'generic',
          'enabled': True,
          'name': f'remote-{i}',
          'mapping': {'key:115': 'accelCruise', 'key:115@long': 'accelCruiseLong', 'key:116': 'laneLeft', 'key:117': 'laneRight', 'key:118': 'none'},
        }
        for i, mac in enumerate(MACS)
      }
    }
  )


def step(now: float) -> Step:
  return {
    'now': now,
    'config': config(),
    'learning': '{}',
    'available': dict(zip(('input-0', 'input-1'), MACS, strict=True)),
    'errors': {},
    'snapshot': default_state(),
    'reads': {},
  }


def key(code: int, pressed: int, at: float) -> list[Event]:
  sec = math.floor(at)
  usec = round((at - sec) * 1e6)
  stamp = sec + usec / 1e6
  return [{'kind': 1, 'code': code, 'value': pressed, 'at': stamp}, {'kind': 0, 'code': 0, 'value': 0, 'at': stamp}]


def scenarios() -> list[Scenario]:
  import json

  result: list[Scenario] = []
  for stop in ('release', 'disconnect', 'brake', 'gas', 'gear', 'physical', 'disable', 'can', 'offroad', 'stale', 'car-invalid'):
    steps = [step(now) for now in (10.0, 10.71, 11.22, 11.23, 11.8, 12.4)]
    steps[0]['reads']['input-0'] = key(115, 1, 10.0)
    for item in steps[3:]:
      match stop:
        case 'release':
          if item is steps[3]:
            item['reads']['input-0'] = key(115, 0, 11.23)
        case 'disconnect':
          item['available'].pop('input-0')
        case 'brake':
          item['snapshot']['brake'] = True
        case 'gas':
          item['snapshot']['gas'] = True
        case 'gear':
          item['snapshot']['gear_drive'] = False
        case 'physical':
          item['snapshot']['physical_buttons'] = True
        case 'disable':
          item['snapshot']['enabled'] = False
        case 'can':
          item['snapshot']['can_valid'] = False
        case 'offroad':
          item['snapshot']['started'] = False
        case 'stale':
          item['snapshot']['controls_alive'] = False
        case 'car-invalid':
          item['snapshot']['car_valid'] = False
        case unreachable:
          assert_never(unreachable)
    result.append((f'hold-{stop}', steps))
  for learning in (
    '{}',
    f'{{"address":"{MACS[0]}","until":Infinity}}',
    f'{{"address":"{MACS[0]}","until":NaN}}',
    f'{{"address":"{MACS[0]}","until":10.25}}',
    '[]',
    '{"until":true}',
    '{broken',
  ):
    steps = [step(10 + i * 0.25) for i in range(8)]
    for item in steps:
      item['learning'] = learning
      item['reads'] = {path: key(116, 1, item['now']) + key(116, 0, item['now']) for path in item['available']}
    result.append((f'learning-{len(result)}', steps))
  steps = [step(10 + i * 0.3) for i in range(12)]
  for i, item in enumerate(steps):
    settings = json.loads(item['config'])
    if i in (1, 2):
      settings['devices'][MACS[0]]['enabled'] = False
    if i in (3, 4):
      settings['devices'][MACS[0]]['name'] = 'renamed'
    if i == 5:
      settings['devices'][MACS[0]]['mapping'] = {'key:116': 'laneRight'}
    if i == 6:
      settings['devices'][MACS[0]]['profile'] = 'yiser-j6'
    item['config'] = json.dumps(settings)
    if i == 7:
      item['available']['input-0'] = MACS[1]
    if i == 8:
      item['available'].pop('input-0')
    if i == 9:
      item['errors']['input-0'] = 'fixture open failure'
    item['reads'] = {path: key(116, 1, item['now']) + key(116, 0, item['now']) for path in item['available']}
  result.append(('reload-reconnect', steps))
  steps = [step(10 + i * 0.025) for i in range(180)]
  for i, item in enumerate(steps):
    code = 116 + i % 3
    item['reads'] = {path: key(code, 1, item['now']) + key(code, 0, item['now']) for path in item['available']}
    item['snapshot']['enabled'] = False
    item['snapshot']['v_ego'] = (0.099, 0.1, -0.099, -0.1)[i % 4]
  result.append(('throttle-history', steps))
  for age in (-0.01, 0.0, 0.399999, 0.4, 0.400001):
    steps = [step(10.0), step(10.3), step(10.6)]
    steps[0]['reads']['input-0'] = key(116, 1, 10 - age) + key(116, 0, 10 - age)
    steps[1]['reads']['input-0'] = key(116, 1, 10.3) + key(116, 0, 10.3)
    result.append((f'input-age-{age}', steps))
  steps = [step(now) for now in (10.0, 10.1, 10.19, 10.2, 10.25, 10.3, 10.5, 10.75)]
  steps[1]['reads']['input-0'] = []
  for item in steps[4:]:
    item['config'] = 'not JSON'
  result.append(('disconnect-invalid-config', steps))
  return result

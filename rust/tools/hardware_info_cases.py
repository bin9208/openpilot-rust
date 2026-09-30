# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Synthetic source/native hardware scenarios; imported by check_hardware_info.py."""

from dataclasses import dataclass
import itertools
import json
from typing import TypeAlias

Json: TypeAlias = bool | int | float | str | None | list["Json"] | dict[str, "Json"]
Step: TypeAlias = dict[str, Json]


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  steps: tuple[Step, ...]
  hardware: str = 'tici'
  wire: tuple[tuple[str, tuple[bytes, ...]], ...] = ()
  pc: bool | None = None
  darwin: bool = False
  endpoint_file: bool = False


def call(method: str, **kwargs: Json) -> Step:
  return {'action': 'call', 'method': method, **kwargs}


def write(path: str, data: str | bytes) -> Step:
  return {'action': 'write', 'path': path, 'bytes': list(data.encode() if isinstance(data, str) else data)}


def directory(path: str) -> Step:
  return {'action': 'directory', 'path': path}


def remove(path: str) -> Step:
  return {'action': 'remove', 'path': path}


def chmod(path: str, mode: int) -> Step:
  return {'action': 'chmod', 'path': path, 'mode': mode}


MODEL = '/sys/firmware/devicetree/base/model'
MODEM = '/dev/shm/modem'
THERMAL = '/sys/devices/virtual/thermal'
ROUTE = '/proc/net/route'
VOLTAGE = '/sys/class/hwmon/hwmon1/in1_input'
CURRENT = '/sys/class/hwmon/hwmon1/curr1_input'
POWER = '/sys/class/hwmon/hwmon1/power1_input'
SOM_VOLTAGE = '/sys/class/power_supply/bms/voltage_now'
SOM_CURRENT = '/sys/class/power_supply/bms/current_now'
GPU = '/sys/class/kgsl/kgsl-3d0/gpubusy'
MAX_BRIGHT = '/sys/class/backlight/panel0-backlight/max_brightness'
BRIGHT = '/sys/class/backlight/panel0-backlight/brightness'
NM = '/run/NetworkManager/system-connections'
NM_DATA = '/data/etc/NetworkManager/system-connections'
ENCODER = '/sys/kernel/debug/msm_vidc/core0/info'
BASIC = (
  'device_type',
  'os_version',
  'serial',
  'modem_state',
  'network_type',
  'sim_info',
  'network_info',
  'modem_version',
  'modem_temperatures',
  'current_power',
  'som_power',
  'brightness',
  'gpu_usage',
  'networks',
  'modem_usage',
  'voltage',
  'current',
  'internal_panda',
  'booted',
)
MODEM_CALLS = ('modem_state', 'network_type', 'sim_info', 'network_info', 'modem_version', 'modem_temperatures', 'modem_usage', 'networks')


def build_cases() -> list[Case]:
  cases = []
  for kind in ('base', 'pc', 'tici'):
    steps = [call(method) for method in BASIC]
    steps += [call('imei', slot=slot) for slot in (0, 1, 99)]
    steps += [call(method, network=network) for method, network in itertools.product(('strength', 'metered'), range(8))]
    steps += [call('thermal_config')]
    if kind != 'tici':
      steps.append(call('thermal_read'))
    cases.append(Case('missing-' + kind, tuple(steps), hardware=kind))
  steps = [
    call('device_type'),
    write(MODEL, b'\xff'),
    call('device_type'),
    write(MODEL, b'\0comma tici\0'),
    call('device_type'),
    write(MODEL, b'comma mici\0'),
    {'action': 'new_instance'},
    call('device_type'),
    remove(MODEL),
    call('device_type'),
  ]
  cases.append(Case('model-cache-success-only', tuple(steps)))
  for index, model in enumerate(
    [b'', b'comma mici\0', b'comma mici\n\0', b'first comma tici comma tizi\0', b'\0comma tici\0\0', b'comma ', 'comma 한국'.encode()]
  ):
    cases.append(Case(f'model-{index:02}', (write(MODEL, model), call('device_type'), call('network_info'), call('thermal_config'))))
  steps = []
  for data in [
    b'',
    b'bare key=first androidboot.serialno=serial\n',
    b'androidboot.serialno=a androidboot.serialno=bad=tail androidboot.serialno=b\r\n',
    b'=empty-key androidboot.serialno= x=y\rnext=bad',
    b'androidboot.serialno=\xff',
    'androidboot.serialno=한글'.encode(),
  ]:
    steps += [write('/proc/cmdline', data), call('cmdline'), call('serial')]
  for data in [b'19.8\r\n', b' \t\x1cversion\x1f \n', b'\xff', b'']:
    steps += [write('/VERSION', data), call('os_version')]
  cases.append(Case('serial-os-text-boundaries', tuple(steps)))
  for name, path, method in [
    ('model', MODEL, 'device_type'),
    ('serial', '/proc/cmdline', 'serial'),
    ('os', '/VERSION', 'os_version'),
    ('voltage', VOLTAGE, 'voltage'),
    ('modem', MODEM, 'modem_state'),
  ]:
    steps = [write(path, b'1'), chmod(path, 0), call(method), chmod(path, 0o600), remove(path), directory(path), call(method)]
    cases.append(Case('permission-directory-' + name, tuple(steps)))
  raw_modems = [
    b'',
    b'{',
    b'{"imei":',
    b'null',
    b'[]',
    b'[1]',
    b'1',
    b'true',
    b'false',
    b'"scalar"',
    b'NaN',
    b'Infinity',
    b'-Infinity',
    b'\xff',
    b'\xff\xfe{\x00}',
    b'{"imei":"\\ud800"}',
    b'{"imei":"old","imei":true}',
    b'{"number":' + b'9' * 4301 + b'}',
    b'{"number":1e9999}',
  ]
  for index, raw in enumerate(raw_modems):
    steps = [write(MODEL, b'comma tici\0'), write(MODEM, raw)] + [call(method) for method in MODEM_CALLS]
    steps += [call('imei', slot=0), call('imei', slot=1), call('strength', network=4)]
    cases.append(Case(f'modem-root-{index:02}', tuple(steps)))
  scalar_values: list[Json] = [
    None, False, True, 0, 1, -1, '', 'lte', 'straße', '\u0264', '\u1c8a', [], [1], {}, {'key': 1},
    float('nan'), float('inf'), -float('inf'), '\ud800',
  ]
  keys = (
    'iccid',
    'sim_state',
    'mcc_mnc',
    'connected',
    'imei',
    'network_type',
    'operator',
    'band',
    'channel',
    'extra',
    'state',
    'modem_version',
    'temperatures',
    'tx_bytes',
    'rx_bytes',
    'signal_quality',
  )
  steps = [write(MODEL, b'comma tici\0')]
  for value in scalar_values:
    steps += [write(MODEM, json.dumps(dict.fromkeys(keys, value))), *[call(method) for method in MODEM_CALLS], call('imei'), call('strength', network=4)]
  steps += [write(MODEM, '{"imei":' + '9' * 4300 + ',"tx_bytes":-' + '9' * 4300 + '}'), call('imei'), call('modem_usage')]
  cases.append(Case('modem-scalar-types', tuple(steps)))
  steps = [write(MODEL, b'comma mici\0'), directory(MODEM), call('network_info'), call('networks'), call('imei', slot=1), call('imei', slot=0)]
  cases.append(Case('mici-and-imei-short-circuit', tuple(steps)))
  for index, quality in enumerate(
    [None, False, True, -100, 0, 24.999, 25, 49.999, 50, 74.999, 75, 999, '25', [], {}, float('nan'), float('inf'), -float('inf')]
  ):
    quality_json = json.dumps(quality)
    steps = [write(MODEM, '{"signal_quality":' + quality_json + '}'), call('parse_strength', value_json=quality_json), call('strength', network=4)]
    cases.append(Case(f'signal-quality-{index:02}', tuple(steps)))
  headers = 'Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n'
  routes = [
    '',
    headers,
    headers + 'wlan0 00000000 00 0001 0 0 10 0\n',
    headers + 'eth1 00000000 00 0001 0 0 1 0\n',
    headers + 'wlan0 00000000 0 1 0 0 10\neth0 00000000 0 1 0 0 10\n',
    headers + 'wwan0 00000000 0 1 0 0 -2\nwlan0 00000000 0 1 0 0 3\n',
    headers + 'x not-default\nwlan0 00000000 0 0 bad\n',
    headers + '\n',
    headers + 'short\n',
    headers + 'eth0 00000000 0 bad 0 0 1\n',
    headers + 'eth0 00000000 0 1 0 0 bad\n',
    headers + 'eth0 00000000 0 -1 0 0 0_1\n',
    headers + 'wlan5 00000000 0 0x_1 0 0 ١٢\n',
    headers + 'ethZ 00000000 0 1 0 0 ' + '9' * 4300 + '\nwlanZ 00000000 0 1 0 0 3\n',
  ]
  steps = [write(MODEM, '{"connected":true,"network_type":"lte"}')]
  for route in routes:
    steps += [write(ROUTE, route), call('route'), call('network_type')]
  steps += [write(ROUTE, b'\xff'), call('route'), call('network_type'), chmod(ROUTE, 0), call('route'), call('network_type'), chmod(ROUTE, 0o600)]
  cases.append(Case('default-route-selection-errors', tuple(steps)))
  steps = [write(ROUTE, headers)]
  for connected, network in itertools.product([False, True, [], [1], '', 'yes'], ['nr', 'lte', 'umts', 'utran', 'gsm', 'unknown', None, 4]):
    steps += [write(MODEM, json.dumps({'connected': connected, 'network_type': network})), call('network_type')]
  cases.append(Case('cellular-network-map-truthiness', tuple(steps)))
  steps = []
  for raw in [b'', b'0', b'1', b'true', b'\xff']:
    steps += [write('/params/d/GsmMetered', raw)] + [call('metered', network=network) for network in range(8) if network != 1]
  steps += [
    chmod('/params/d/GsmMetered', 0),
    call('metered', network=4),
    chmod('/params/d/GsmMetered', 0o600),
    remove('/params/d/GsmMetered'),
    directory('/params/d/GsmMetered'),
    call('metered', network=4),
  ]
  cases.append(Case('gsm-params-exact-bool', tuple(steps)))
  _paths_cases(cases)
  _socket_cases(cases)
  _keyfile_cases(cases)
  _numeric_cases(cases)
  _thermal_cases(cases)
  _scan_cases(cases)
  return cases


def _paths_cases(cases: list[Case]) -> None:
  homes = [None, b'', b'/', b'//', b'///', b'.', b'folder//./child/', b'//server///folder/', b'/a/../b', b'/raw-\xff']
  prefixes = [None, b'', b'-test', b'/a//./b', b'/../parent', b'-\xff']
  overrides = [None, b'', b'/override//tail/']
  for pc, darwin in itertools.product([False, True], repeat=2):
    steps = []
    for home, prefix, log, cache in itertools.product(homes, prefixes, overrides, overrides):
      values = {'HOME': home, 'OPENPILOT_PREFIX': prefix, 'LOG_ROOT': log, 'COMMA_CACHE': cache}
      steps += [{'action': 'env', 'values': {name: list(value) if value is not None else None for name, value in values.items()}}, call('paths')]
    cases.append(Case(f'paths-pc-{int(pc)}-darwin-{int(darwin)}', tuple(steps), hardware='pc', pc=pc, darwin=darwin))
  cases.append(Case('paths-actual-environment', (call('paths'),), hardware='pc'))


def _socket_cases(cases: list[Case]) -> None:
  payloads = [
    (b'key=old\nkey=new\nssid=test\nempty=\n=keyless\nnoequal\n',),
    (b'<3>event\n', b'<2>another', b'ssid=test\nRSSI=-50\n'),
    (b'FAIL\n',),
    (b'FAILUREextra',),
    (b'',),
    (b'k=\xff\xc3\xa9\r\nx=1\xc2\x85y=2\xe2\x80\xa8z=3\x1fq=4\n',),
    (b'key=' + b'x' * 9000,),
    (),
  ]
  for index, replies in enumerate(payloads):
    cases.append(Case(f'wpa-datagram-{index:02}', (call('wpa'),), wire=(('STATUS', replies),)))
  cases.append(Case('wpa-unsolicited-then-timeout', (call('wpa'),), wire=(('STATUS', (b'<3>event',)),)))
  cases.append(Case('wpa-nonblocking-no-reply', (call('wpa', timeout_ms=0),), wire=(('STATUS', ()),)))
  cases.append(Case('wpa-missing-endpoint', (call('wpa'),)))
  cases.append(Case('wpa-regular-file-endpoint', (call('wpa'),), endpoint_file=True))
  replies = []
  steps = [call('strength', network=0), call('strength', network=6)]
  for rssi in ['-101', '-100', '-99', '-96', '-95', '-71', '-70', '-46', '-45', '-20', '0', '1', '-9_5', '-٩٥', '  -50 ', '', 'bad', '9' * 4300]:
    replies.append(('SIGNAL_POLL', (f'RSSI={rssi}\n'.encode(),)))
    steps.append(call('strength', network=1))
  for reply in [(b'NO_RSSI=1\n',), (b'FAIL\n',), ()]:
    replies.append(('SIGNAL_POLL', reply))
    steps.append(call('strength', network=1))
  cases.append(Case('wifi-strength-bounds-and-errors', tuple(steps), wire=tuple(replies)))


def _keyfile_cases(cases: list[Case]) -> None:
  keyfiles = [
    '[wifi]\nssid=test\n[connection]\nmetered=1\n',
    '[wifi]\nssid:test\n[connection]\nMETERED:2\n',
    '[wifi]\nssid=test\n[connection]\nmetered=0\n',
    '[wifi]\nssid=test\n[connection]\nmetered=3\n',
    '[wifi]\nssid=test\n[connection]\n',
    '[DEFAULT]\nssid=test\nmetered=1\n[wifi]\n[connection]\n',
    '[DEFAULT]\nssid=test\nmetered=1\n[connection]\n',
    '[wifi]\nssid=test\nSSID=test\n[connection]\nmetered=1\n',
    '[wifi]\nssid=test\n[wifi]\n[connection]\nmetered=1\n',
    'ssid=test\n[connection]\nmetered=1\n',
    '[wifi]\nssid=test\ninvalid-option\n[connection]\nmetered=1\n',
    '[wifi]\nssid=test\n[connection]\nmetered=1 # inline\n',
    '[wifi]\nssid=test\n[connection]\nmetered=0_1\n',
    '[wifi]\nssid=test\n[connection]\nmetered=١\n',
    '[wifi]\nssid=test\n[connection]\nmetered=1\n \n# comment\n',
    '[wifi]\nssid=te\n st\n[connection]\nmetered=1\n',
    '[wifi]ignored suffix\nssid=test\n[connection]\nmetered=1\n',
    '[wifi]\nssid=test\n  [connection]\n  metered=1\n',
    '[wifi]\nssid=other\n[connection]\nmetered=invalid\n',
    '[wifi]\nssid=test\n[connection]\nmetered=' + '9' * 4301 + '\n',
  ]
  for index, keyfile in enumerate(keyfiles):
    steps = [
      write(NM + '/a.nmconnection', keyfile),
      write(NM_DATA + '/b.nmconnection', '[wifi]\nssid=test\n[connection]\nmetered=1\n'),
      call('metered', network=1),
    ]
    cases.append(Case(f'keyfile-parser-{index:02}', tuple(steps), wire=(('STATUS', (b'ssid=test\n',)),)))
  for index, (ssid, key) in enumerate(
    [
      ('café', 'café'),
      ('café', '99;97;102;195;169;'),
      (r'caf\xc3\xa9', '99;97;102;195;169;'),
      (r'\u00e9', '233;'),
      (r'\N{LATIN SMALL LETTER E WITH ACUTE}', '233;'),
      (r'\N{low line}', '95;'),
      (r'\N{LF}', '10;'),
      (r'\N{low_line}', r'\N{low_line}'),
      (r'\377', '255;'),
      (r'\400', r'\400'),
      (r'\z', '92;122;'),
      (r'\x0', r'\x0'),
      (r'\u0301', r'\u0301'),
      ('😀', '240;159;152;128;'),
      (r'\U0001f600', r'\U0001f600'),
      (r'a\\b', '97;92;98;'),
      (r'percent%name', 'percent%name'),
    ]
  ):
    steps = [write(NM + '/a.nmconnection', f'[wifi]\nssid={key}\n[connection]\nmetered=1\n'), call('metered', network=1)]
    cases.append(Case(f'ssid-bytes-{index:02}', tuple(steps), wire=(('STATUS', (f'ssid={ssid}\n'.encode(),)),)))
  steps = [
    write(NM + '/a.nmconnection', '[wifi]\nssid=test\n[connection]\nmetered=1\n'),
    chmod(NM + '/a.nmconnection', 0),
    directory(NM + '/b.nmconnection'),
    write(NM + '/ignored.NMCONNECTION', '[wifi]\nssid=test\n[connection]\nmetered=1\n'),
    write(NM_DATA + '/c.nmconnection', '[wifi]\nssid=test\n[connection]\nmetered=2\n'),
    call('metered', network=1),
    chmod(NM + '/a.nmconnection', 0o600),
  ]
  cases.append(Case('keyfile-read-errors', tuple(steps), wire=(('STATUS', (b'ssid=test\n',)),)))
  steps = [
    directory(NM),
    chmod(NM, 0),
    write(NM_DATA + '/a.nmconnection', '[wifi]\nssid=test\n[connection]\nmetered=1\n'),
    call('metered', network=1),
    chmod(NM, 0o700),
  ]
  cases.append(Case('keyfile-unreadable-first-directory', tuple(steps), wire=(('STATUS', (b'ssid=test\n',)),)))


def _numeric_cases(cases: list[Case]) -> None:
  steps = []
  for raw in [b'123\n', b'-42', b'  +1_234 \r\n', ' ١٢٣\u0085'.encode(), b'', b'1.25', b'0x10', b'1\x1c', b'\xff', b'9' * 309, b'9' * 4300, b'9' * 4301]:
    steps += [write(VOLTAGE, raw), write(CURRENT, raw), write(POWER, raw), write(SOM_VOLTAGE, raw), write(SOM_CURRENT, b'2')]
    steps += [call(method) for method in ('voltage', 'current', 'current_power', 'som_power')]
  steps += [write(SOM_VOLTAGE, b'9' * 4300), write(SOM_CURRENT, b'0'), call('som_power')]
  cases.append(Case('physical-integer-parsing-overflow', tuple(steps)))
  steps = []
  for raw in [
    b'10 20',
    b'-5 10',
    b'30 20',
    b'1 0',
    b'1 -1',
    b'1',
    b'1 2 3',
    b'\xff',
    b'1_0 2_0',
    '١٠\u0085٢٠'.encode(),
    b'9' * 309 + b' 1',
    b'9' * 307 + b' 1',
  ]:
    steps += [write(GPU, raw), call('gpu_usage')]
  cases.append(Case('gpu-percent-catches-and-unbounded-values', tuple(steps)))
  steps = []
  for maximum, brightness in [
    ('1000', '555'),
    ('1000', '-12.7'),
    ('1000', '2500'),
    ('0', '1'),
    ('NaN', '2'),
    ('Infinity', '10'),
    ('-Infinity', '10'),
    ('5e-324', '100'),
    ('1000', 'NaN'),
    ('1000', 'Infinity'),
    ('1000', '9e300'),
    ('1_000', '٥٥٥'),
    (' 1000\x1c', '555'),
    ('1000', '555\x1c'),
    ('bad', '1'),
  ]:
    steps += [write(MAX_BRIGHT, maximum), write(BRIGHT, brightness), call('brightness')]
  cases.append(Case('brightness-float-conversion', tuple(steps)))
  steps = []
  for state, uptime in itertools.product(['Core state: 0', 'Core state: 1', ''], [0, 119.999, 120, 121]):
    steps += [write(ENCODER, state), call('booted', uptime=uptime)]
  cases.append(Case('encoder-booted-boundary', tuple(steps)))


def _thermal_cases(cases: list[Case]) -> None:
  zone = {'action': 'zone', 'name': 'z', 'zone': 'cpu0-silver-usr'}
  read = call('zone_read', name='z')
  steps = [
    zone,
    read,
    directory(THERMAL),
    read,
    write(THERMAL + '/thermal_zone0/type', 'other\n'),
    read,
    write(THERMAL + '/thermal_zone0/type', 'cpu0-silver-usr\n'),
    read,
    write(THERMAL + '/thermal_zone0/temp', '21000'),
    read,
    write(THERMAL + '/thermal_zone0/type', 'renamed'),
    write(THERMAL + '/thermal_zone0/temp', '42000'),
    read,
    remove(THERMAL + '/thermal_zone0'),
    write(THERMAL + '/thermal_zone1/type', 'cpu0-silver-usr'),
    write(THERMAL + '/thermal_zone1/temp', '99000'),
    read,
  ]
  cases.append(Case('thermal-discovery-cache-lifetime', tuple(steps)))
  steps = [
    zone,
    write(THERMAL + '/thermal_zone0/type', 'cpu0-silver-usr'),
    write(THERMAL + '/thermal_zone0/temp', 'bad'),
    read,
    write(THERMAL + '/thermal_zone0/temp', b'\xff'),
    read,
    write(THERMAL + '/thermal_zone0/temp', '12345'),
    chmod(THERMAL + '/thermal_zone0/temp', 0),
    read,
    chmod(THERMAL + '/thermal_zone0/temp', 0o600),
    read,
  ]
  cases.append(Case('thermal-temp-errors-not-hidden', tuple(steps)))
  for index, type_value in enumerate([None, b'\xff', b'cpu0-silver-usr']):
    steps = [zone, directory(THERMAL + '/thermal_zone0')]
    if type_value is not None:
      steps += [write(THERMAL + '/thermal_zone0/type', type_value)]
    if index == 2:
      steps.append(chmod(THERMAL + '/thermal_zone0/type', 0))
    steps.append(read)
    if index == 2:
      steps.append(chmod(THERMAL + '/thermal_zone0/type', 0o600))
    cases.append(Case(f'thermal-type-error-{index}', tuple(steps)))
  for index, scale in enumerate(['0', '-1000', 'NaN', 'Infinity', '-Infinity']):
    steps = [
      {**zone, 'scale_json': scale},
      write(THERMAL + '/thermal_zone0/type', 'cpu0-silver-usr'),
      read,
      write(THERMAL + '/thermal_zone0/temp', '1000'),
      read,
    ]
    cases.append(Case(f'thermal-scale-{index}', tuple(steps)))
  for suffix in ['bad', '+02', '-2']:
    steps = [
      zone,
      write(THERMAL + '/thermal_zone' + suffix + '/type', 'cpu0-silver-usr'),
      write(THERMAL + '/thermal_zone' + suffix + '/temp', '12000'),
      read,
      read,
    ]
    cases.append(Case('thermal-suffix-' + suffix, tuple(steps)))
  names = [f'cpu{i}-silver-usr' for i in range(4)] + [f'cpu{i}-gold-usr' for i in range(4)]
  names += ['gpu0-usr', 'gpu1-usr', 'compute-hvx-usr', 'ddr-usr', 'pm8998_tz', 'pm8005_tz', 'intake', 'exhaust', 'gnss', 'bottom_soc']
  for model in ['tici', 'mici']:
    steps = [write(MODEL, 'comma ' + model + '\0')]
    for index, name in enumerate(names):
      steps += [write(f'{THERMAL}/thermal_zone{index}/type', name), write(f'{THERMAL}/thermal_zone{index}/temp', str((index + 1) * 1000))]
    steps += [call('thermal_config'), call('thermal_read'), write(THERMAL + '/thermal_zone0/temp', '1234'), call('thermal_read')]
    cases.append(Case('thermal-full-config-' + model, tuple(steps)))


def _scan_cases(cases: list[Case]) -> None:
  outputs = [
    'Cell 01 - Address: AA:BB\n Quality=70 Signal level=-40 dBm\nCell 02 - Address: CC:DD\n',
    'Signal level=-50 dBm\n',
    'Cell Address: AA\n level dBm\n',
    'Cell Address: AA\n Signal level=bad dBm\nCell Address: BB\n Signal level=-8_0 dBm\n',
    'Cell Address: AA \n',
    '',
  ]
  extras: list[Json] = [
    'state,"LTE",1,450,08,00ab,23,1300',
    'LTE,short',
    'state,"LTE",1,bad,1,0,1,2',
    'state,"LTE",1,١_٢,3,+0x_10,4,5',
    'none',
    None,
    False,
    1,
    [],
    ['LTE'],
    {},
    {'LTE': True},
    'surrogate\ud800,"LTE",1,450,08,00ab,23,1300',
    'state,"LTE",1,\ud800,1,0,1,2',
  ]
  steps = [write(MODEL, 'comma tici\0')]
  for output in outputs:
    steps += [write('/commands/iwlist.stdout', output), write(MODEM, '{"network_type":"lte","extra":"none"}'), call('networks')]
  for extra in extras:
    steps += [write(MODEM, json.dumps({'network_type': 'lte', 'extra': extra})), call('networks')]
  steps += [write('/commands/iwlist.stdout', b'\xff'), call('networks'), write('/commands/iwlist.status', '3\n'), call('networks')]
  cases.append(Case('iwlist-and-lte-extra', tuple(steps)))

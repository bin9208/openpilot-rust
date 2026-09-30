# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "pyzmq==27.2.0", "numpy==2.5.3", "setproctitle==1.3.7"]
# ///
"""Compare full manager registration and ordered callbacks to unchanged source."""

import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import tomllib

PREDICATES = [
  'driverview',
  'notcar',
  'iscar',
  'logging',
  'ublox',
  'joystick',
  'not_joystick',
  'long_maneuver',
  'lat_maneuver',
  'not_long_maneuver',
  'qcomgps',
  'always_run',
  'only_onroad',
  'only_offroad',
  'enable_updated',
  'enable_dm',
  'enable_xiaoge_data',
  'enable_webrtc',
  'c3x_lite',
  'enable_youtube_low_encoder',
  'enable_youtube_medium_encoder',
  'enable_youtube_encoder',
  'enable_youtube_wide_encoder',
  'enable_cluster_hud',
]
BOOL_KEYS = [
  'IsDriverViewEnabled',
  'DisableLogging',
  'UbloxAvailable',
  'JoystickDebugMode',
  'LongitudinalManeuverMode',
  'LateralManeuverMode',
  'SoftwareMenu',
  'ShareData',
  'HardwareC3xLite',
]
INTEGER_KEYS = ['DisableDM', 'ClusterHud', 'CarrotYouTubeLive', 'CarrotYouTubeQuality']


def encoded(values):
  return {key: list(value if isinstance(value, bytes) else str(value).encode()) for key, value in values.items()}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('binding', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.binary, args.binding, args.output = (path.resolve() for path in (args.binary, args.binding, args.output))
  args.output.mkdir(parents=True, exist_ok=True)
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  provenance = json.loads(args.binding.with_name('provenance.json').read_text())
  assert hashlib.sha256(args.binding.read_bytes()).hexdigest() == provenance['module_sha256']
  for source, digest in provenance['sources'].items():
    assert hashlib.sha256(Path(source).read_bytes()).hexdigest() == digest, source
  rows = []
  env = dict(os.environ)
  env.pop('OPENPILOT_PREFIX', None)

  def run(name, request, extra_env=None, fatal=False):
    folder = args.output / name
    folder.mkdir()
    path = folder / 'request.json'
    path.write_text(json.dumps(request, indent=2) + '\n')
    outputs = {}
    invocations = {}
    for kind in ['source', 'native']:
      out = folder / kind
      command = ([sys.executable, 'rust/tools/manager_catalog_reference.py', str(args.binding)] if kind == 'source' else [*args.runner, str(args.binary)]) + [
        str(path),
        str(out),
      ]
      current_env = {**env, **(extra_env or {})}
      for key in ['USE_WEBCAM', 'CARROT_WEB_EXTERNAL']:
        if extra_env and extra_env.get(key) is None:
          current_env.pop(key, None)
      result = subprocess.run(command, env=current_env, capture_output=True, timeout=600)
      (folder / f'{kind}.stdout').write_bytes(result.stdout)
      (folder / f'{kind}.stderr').write_bytes(result.stderr)
      invocations[kind] = {'argv': command, 'exit_code': result.returncode}
      if kind == 'source' and fatal:
        assert result.returncode == -signal.SIGABRT, (name, result.returncode, result.stderr)
        outputs[kind] = json.loads((out / 'trace-0.json').read_text())
      else:
        assert result.returncode == 0, (name, kind, result.returncode, result.stderr.decode(errors='replace'))
        outputs[kind] = json.loads((out / 'result.json').read_text())
    if fatal:
      native = outputs['native']['results'][0]
      assert native['outcome'] == {'error': 'fatal_integer'}, (name, native)
      assert outputs['source'] == native['trace'], (name, outputs['source'], native)
      observable = 'source SIGABRT; native typed fatal_integer; identical access prefix; never false/default'
    else:
      for key in ['catalog', 'results']:
        assert outputs['source'][key] == outputs['native'][key], (
          name,
          key,
          next(((i, a, b) for i, (a, b) in enumerate(zip(outputs['source'][key], outputs['native'][key], strict=False)) if a != b), None),
        )
      assert len(outputs['source']['catalog']) == 63
      assert len({item['name'] for item in outputs['source']['catalog']}) == 63
      assert 'uploader' not in {item['name'] for item in outputs['source']['catalog']}
      observable = f"63 ordered descriptors and {len(request.get('cases', []))} callback outcomes/access traces/persisted Ublox values identical"
    row = {
      'scenario': name,
      'invocations': invocations,
      'observable': observable,
      'artifacts': [str(path), str(folder / 'native/result.json'), str(folder / ('source/trace-0.json' if fatal else 'source/result.json'))],
    }
    rows.append(row)
    (folder / 'comparison.json').write_text(json.dumps(row, indent=2) + '\n')
    return outputs['native']

  names = None
  for index, flags in enumerate(itertools.product([False, True], repeat=6)):
    config = dict(zip(['pc', 'tici', 'webcam', 'carrot_web_external', 'darwin', 'bodyteleop_available'], flags, strict=True))
    result = run(f'catalog-{index:02}', {'config': config})
    names = [item['name'] for item in result['catalog']]
  for index, (webcam, external) in enumerate(itertools.product([None, '', '0', '1', '\udcff'], repeat=2)):
    run(f'environment-{index:02}', {'bodyteleop_available': True}, {'USE_WEBCAM': webcam, 'CARROT_WEB_EXTERNAL': external})
  run('body-module-missing', {'body_discovery_missing_parent': True, 'bodyteleop_available': False})
  cases = []

  def add(predicate, values=None, **kw):
    cases.append({'predicate': predicate, 'started': True, 'not_car': False, 'values': encoded(values or {}), **kw})

  profiles = [{}, dict.fromkeys(BOOL_KEYS, 1), dict.fromkeys(BOOL_KEYS, 0)]
  profiles += [
    {'DisableDM': dm, 'ClusterHud': hud, 'CarrotYouTubeLive': 1, 'CarrotYouTubeQuality': quality}
    for dm, hud, quality in itertools.product([0, 1, 2], [0, 1], [0, 1, 2, 3, 4])
  ]
  for profile, started, not_car in itertools.product(profiles, [False, True], [False, True]):
    for name in names:
      add('process:' + name, profile, started=started, not_car=not_car)
  run('full-catalog-policy', {'cases': cases})
  count = len(cases)
  cases = []
  for name, started, not_car in itertools.product(PREDICATES, [False, True], [False, True]):
    add(name, started=started, not_car=not_car)
  for key, value in itertools.product(BOOL_KEYS, [b'', b'0', b'1', b'true', b'1\x00', b'\xff']):
    for name in PREDICATES:
      add(name, {key: value})
  for dm, driver in itertools.product([-1, 0, 1, 2, 3], [0, 1]):
    for started in [False, True]:
      add('enable_dm', {'DisableDM': dm, 'IsDriverViewEnabled': driver}, started=started)
  for name, live, quality in itertools.product(PREDICATES[-5:-1], [-1, 0, 1, 2], [-1, 0, 1, 2, 3, 4]):
    add(name, {'CarrotYouTubeLive': live, 'CarrotYouTubeQuality': quality})
  for wide in [b'', b'0', b'1', b'true', b'\xff', b'1\x00']:
    add('enable_youtube_wide_encoder', {'UseWideCamera': wide, 'CarrotYouTubeLive': 1, 'CarrotYouTubeQuality': 3})
  for name, started, stored, tty, quectel in itertools.product(['ublox', 'qcomgps'], [False, True], [0, 1], [False, True], [False, True]):
    paths = [path for path, present in [('/dev/ttyHS0', tty), ('/persist/comma/use-quectel-gps', quectel)] if present]
    add(name, {'UbloxAvailable': stored}, started=started, gps_paths=paths)
  affected = {
    'DisableDM': 'enable_dm',
    'ClusterHud': 'enable_cluster_hud',
    'CarrotYouTubeLive': 'enable_youtube_encoder',
    'CarrotYouTubeQuality': 'enable_youtube_encoder',
  }
  for key, value in itertools.product(INTEGER_KEYS, [b'', b'  +2tail', b'-1suffix', b'2147483647', b'-2147483648', b'0\x00garbage', b'1.9', b'\v2']):
    add(affected[key], {'CarrotYouTubeLive': 1, key: value})
  for key in BOOL_KEYS + INTEGER_KEYS + ['UseWideCamera']:
    for name in PREDICATES:
      add(name, {'CarrotYouTubeLive': 1, 'CarrotYouTubeQuality': 3}, directories=[key])
      add(name, {'CarrotYouTubeLive': 1, 'CarrotYouTubeQuality': 3}, exception_key=key)
  # Invalid integer bytes behind short-circuits must never reach Cython stoi.
  for name in ['enable_dm', 'process:carrot_vision_encoderd', 'process:youtube_encoderd', 'process:youtube_wide_encoderd']:
    add(name, dict.fromkeys(INTEGER_KEYS, b'invalid'), started=False)
  add('enable_youtube_wide_encoder', {'UseWideCamera': 0, 'CarrotYouTubeLive': b'invalid'})
  add('enable_youtube_encoder', {'CarrotYouTubeLive': 0, 'CarrotYouTubeQuality': b'invalid'})
  add('enable_webrtc', {'DisableDM': 0, 'ClusterHud': b'invalid'})
  run('parameter-and-order-policy', {'cases': cases})
  count += len(cases)
  fatal_count = 0
  for key, value in itertools.product(INTEGER_KEYS, [b'invalid', b'2147483648', b'-2147483649', b' ', b'\xff', b'\x00']):
    request = {'cases': [{'predicate': affected[key], 'started': True, 'not_car': False, 'values': encoded({'CarrotYouTubeLive': 1, key: value})}]}
    run(f'fatal-{fatal_count:02}', request, fatal=True)
    fatal_count += 1
  inventory = json.loads(Path('rust/port-status.json').read_text())
  statuses = {item['name']: item['status'] for item in inventory['processes']}
  for item in result['availability']:
    assert (item['status'] == 'not_ported') == (statuses[item['name']] == 'not_ported'), item
    if item['status'] == 'candidate':
      manifests = [tomllib.loads(path.read_text()) for path in Path('rust/crates').glob('*/Cargo.toml')]
      package = next(m for m in manifests if m['package']['name'] == item['package'])
      assert item['binary'] in [b['name'] for b in package.get('bin', [])] or item['binary'] == item['package'], item
      assert item['limitation']
  sources = [
    'openpilot/system/manager/process_config.py',
    'openpilot/system/manager/process.py',
    'openpilot/common/params_pyx.pyx',
    'openpilot/common/params.cc',
    'openpilot/common/params.h',
  ]
  report = {
    'result': 'PASS',
    'descriptors': 63,
    'catalog_configurations': 64,
    'environment_snapshots': 25,
    'predicate_cases': count,
    'fatal_cases': fatal_count,
    'comparisons': rows,
    'source_sha256': {name: hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in sources},
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest(),
    'limits': 'Policy API only; native typed fatal errors are not SIGABRT parity. No manager launch, AGNOS or vehicle validation.',
  }
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(
    json.dumps({key: report[key] for key in ['result', 'descriptors', 'catalog_configurations', 'environment_snapshots', 'predicate_cases', 'fatal_cases']})
  )


if __name__ == '__main__':
  main()

"""Original API/UI cache, synthetic local keys, verified native JWT signatures."""

import argparse
from datetime import datetime, timedelta, UTC
import importlib
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType, SimpleNamespace

import jwt

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root))
persist = args.output / 'synthetic-persist'
keys = persist / 'comma'
keys.mkdir(parents=True, exist_ok=True)
for name, command in [
  ('id_rsa', ['genpkey', '-algorithm', 'RSA', '-pkeyopt', 'rsa_keygen_bits:2048']),
  ('id_ecdsa', ['ecparam', '-name', 'prime256v1', '-genkey', '-noout']),
]:
  private = keys / name
  with (args.output / f'{name}-generation.log').open('w') as log:
    subprocess.run(['openssl', *command, '-out', str(private)], check=True, stdout=log, stderr=log)
    subprocess.run(['openssl', 'pkey', '-in', str(private), '-pubout', '-out', str(private) + '.pub'], check=True, stdout=log, stderr=log)
  private.chmod(0o600)
hw = ModuleType('openpilot.system.hardware.hw')
hw.Paths = SimpleNamespace(persist_root=lambda: str(persist))
sys.modules[hw.__name__] = hw
version = ModuleType('openpilot.system.version')
version.get_version = lambda: 'fixture'
sys.modules[version.__name__] = version
api = importlib.import_module('openpilot.common.api')
helper = importlib.import_module('openpilot.selfdrive.ui.lib.api_helpers')
clock = importlib.import_module('openpilot.common.time_helpers')
current = {}


class FixedDatetime(datetime):
  @classmethod
  def now(cls, tz=None):
    value = datetime.fromtimestamp(current['wall'], UTC)
    return value if tz else value.replace(tzinfo=None)


api.datetime = FixedDatetime
clock.datetime = SimpleNamespace(datetime=FixedDatetime, timedelta=timedelta)
clock.min_date = lambda: datetime(2025, 2, 21)
original_monotonic = helper.time.monotonic
helper.time.monotonic = lambda: current['monotonic'] / 1e9
steps = []
for bucket, identity, wall, pair in [
  (0, 'A', 1790812800.25, False),
  (0, 'A', 0, False),
  (0, 'B', 1790812801.75, False),
  (0, 'A', 1790812802, False),
  (1, 'A', 0, False),
  (0, 'A', 0, False),
  (1, 'A', 1790812803, False),
  (1, 'B', 1740096000, False),
  (1, 'B', 1740096001, False),
  (2, 'B', 2051222400, False),
  (2, 'B', 2051222399, False),
  (3, '', 0, True),
  (3, 'A', 1790812800.75, True),
]:
  steps.append({'identity': identity, 'wall': wall, 'monotonic': bucket * 3600 * 10**9, 'pair': pair})
input_path = args.output / 'input.json'
input_path.write_text(json.dumps(steps))
all_results = []
try:
  for algorithm, name in [('RS256', 'id_rsa'), ('ES256', 'id_ecdsa')]:
    helper._get_token.cache_clear()
    source = []
    for step in steps:
      current = step
      try:
        token = api.Api(step['identity']).get_token({'pair': True}) if step['pair'] else helper.get_token(step['identity'])
        source.append({'token': token})
      except Exception as error:
        source.append({'error': str(error)})
    result = subprocess.run([str(args.binary), str(persist), str(input_path)], capture_output=True, text=True, check=True)
    native = json.loads(result.stdout)
    (args.output / f'{algorithm}-source.json').write_text(json.dumps(source, indent=2))
    (args.output / f'{algorithm}-native.json').write_text(json.dumps(native, indent=2))
    public = (keys / f'{name}.pub').read_text()
    for index, (expected, actual) in enumerate(zip(source, native, strict=True)):
      if 'error' in expected:
        assert actual == expected, (algorithm, index, expected, actual)
      else:
        options = {'verify_exp': False, 'verify_iat': False, 'verify_nbf': False}
        expected_claims = jwt.decode(expected['token'], public, algorithms=[algorithm], options=options)
        actual_claims = jwt.decode(actual['token'], public, algorithms=[algorithm], options=options)
        assert expected_claims == actual_claims, (algorithm, index, expected_claims, actual_claims)
    assert native[0] == native[1] and native[3] == native[5]
    all_results.append({'algorithm': algorithm, 'cases': len(steps), 'claims_and_signatures': 'PASS', 'cache_reuse_after_invalid_clock': 'PASS'})
    if algorithm == 'RS256':
      (keys / 'id_rsa').rename(keys / 'id_rsa.inactive')
finally:
  helper.time.monotonic = original_monotonic
(args.output / 'results.json').write_text(json.dumps(all_results, indent=2))
print(
  'PASS: 26 original/native JWT claims and signatures, RSA preference/EC fallback, hour cache, identity eviction,',
  'failed-refresh retention, validity boundaries and pairing exemption',
)

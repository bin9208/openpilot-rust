from __future__ import annotations

import argparse
from datetime import datetime, UTC
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

import jwt
from check_registration import keys


def cases():
  output = [
    {'name': 'environment', 'environment': 'override-token', 'params': {'PrimeType': b'bad'}},
    {'name': 'environment-empty', 'environment': '', 'params': {'PrimeType': b'1'}},
    {'name': 'public-key', 'params': {'PrimeType': b'0', 'MapboxPublicKey': b'public-token'}},
    {'name': 'missing-type', 'params': {'MapboxPublicKey': b'public-token'}},
    {'name': 'public-key-empty', 'params': {'MapboxPublicKey': b''}},
    {'name': 'public-key-invalid-utf8', 'params': {'MapboxPublicKey': b'\xff'}},
    {'name': 'public-key-missing', 'params': {}},
    {'name': 'signed-rsa', 'params': {'PrimeType': b'1', 'DongleId': b'fixture-dongle'}, 'keys': ['rsa']},
    {'name': 'signed-ec', 'params': {'PrimeType': b'2', 'DongleId': b'fixture-dongle'}, 'keys': ['ec']},
    {'name': 'rsa-precedence', 'params': {'PrimeType': b'1', 'DongleId': b'fixture-dongle'}, 'keys': ['rsa', 'ec']},
    {'name': 'signed-null-identity', 'params': {'PrimeType': b'1'}, 'keys': ['rsa']},
    {'name': 'signed-invalid-identity', 'params': {'PrimeType': b'1', 'DongleId': b'\xff'}, 'keys': ['rsa']},
    {'name': 'signed-missing-key', 'params': {'PrimeType': b'1'}},
    {'name': 'missing-key-unicode', 'params': {'PrimeType': b'1', 'DongleId': '기기\x7f😀'.encode()}},
    {'name': 'signed-missing-public', 'params': {'PrimeType': b'1'}, 'keys': ['rsa'], 'no_public': True},
    {'name': 'signed-invalid-private', 'params': {'PrimeType': b'1'}, 'keys': ['rsa'], 'bad_private': True},
  ]
  for index, value in enumerate((b'', b' +0 trailing', b'-0', b'1junk', b'-1', b'2147483647', b'-2147483648',
                               b'bad', b'2147483648', b'-2147483649')):
    output.append({'name': f'integer-{index}', 'params': {'PrimeType': value, 'MapboxPublicKey': b'public'}, 'keys': ['rsa']})
  return output


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  pairs = keys()
  results = []
  source = Path(__file__).with_name('navd_config_source.py')
  for case in cases():
    compared = []
    for implementation in ('source', 'native'):
      output = (args.output / case['name'] / implementation).resolve()
      prefix = f'navdc_{os.getpid()}'
      params = output / 'params' / prefix
      comma = output / 'persist/comma'
      params.mkdir(parents=True)
      comma.mkdir(parents=True)
      for key, value in case['params'].items():
        (params / key).write_bytes(value)
      for key in case.get('keys', []):
        filename = 'id_rsa' if key == 'rsa' else 'id_ecdsa'
        (comma / filename).write_bytes(b'bad key' if case.get('bad_private') else pairs[key]['private'])
        if not case.get('no_public'):
          (comma / (filename + '.pub')).write_bytes(pairs[key]['public'])
      env = dict(os.environ, PARAMS_ROOT=str(params.parent), OPENPILOT_PREFIX=prefix)
      env.pop('MAPBOX_TOKEN', None)
      if 'environment' in case:
        env['MAPBOX_TOKEN'] = case['environment']
      command = ([sys.executable, str(source), '--binding', str(args.binding.resolve()), '--output', str(output)]
                 if implementation == 'source' else [str(args.binary.resolve())])
      before = datetime.now(UTC).timestamp()
      process = subprocess.run(command, input=json.dumps(str(comma.parent)) + '\n', env=env,
                               text=True, capture_output=True, timeout=15, check=False)
      after = datetime.now(UTC).timestamp()
      (output / 'stdout').write_text(process.stdout)
      (output / 'stderr').write_text(process.stderr)
      result = json.loads(process.stdout) if process.returncode == 0 else {'ok': False, 'returncode': process.returncode}
      (output / 'record.json').write_text(json.dumps(result, indent=2) + '\n')
      result.pop('error', None)
      result.pop('returncode', None)
      if result['ok'] and result['host'] == 'https://maps.comma.ai':
        token = result.pop('token')
        header = jwt.get_unverified_header(token)
        if header['alg'] == 'none':
          assert not case.get('keys') or case.get('no_public'), case['name']
          assert token.endswith('.')
          claims = jwt.decode(token, options={'verify_signature': False})
          payload = jwt.utils.base64url_decode(token.split('.')[1]).decode()
          expected = {key: claims[key] for key in ('identity', 'nbf', 'iat', 'exp')}
          assert payload == json.dumps(expected, separators=(',', ':'))
        else:
          key = 'rsa' if header['alg'] == 'RS256' else 'ec'
          claims = jwt.decode(token, pairs[key]['public'], algorithms=[header['alg']])
        assert before - 1 <= claims['nbf'] <= after, (case['name'], claims, before, after)
        assert claims['iat'] == claims['nbf'] and claims['exp'] - claims['iat'] == 4 * 7 * 24 * 3600
        result.update(header=header, identity=claims['identity'], expiry=claims['exp'] - claims['iat'])
      compared.append(result)
    results.append({'name': case['name'], 'passed': compared[0] == compared[1]})
  root = Path(__file__).resolve().parents[2]
  files = [Path(__file__), source, args.binary, args.binding, root / 'openpilot/common/api.py',
           root / 'openpilot/selfdrive/navd/navd.py', *sorted((root / 'rust/crates/navd').rglob('*.rs'))]
  report = {'status': 'PASS' if all(row['passed'] for row in results) else 'FAIL', 'cases': results,
            'files': {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'status': report['status'], 'cases': len(results), 'failures': [row['name'] for row in results if not row['passed']]}))
  raise SystemExit(report['status'] != 'PASS')


if __name__ == '__main__':
  main()

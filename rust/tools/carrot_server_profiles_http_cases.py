import json
import os
import re
import shutil
import subprocess
from datetime import datetime, timezone

_ids = {}
_marker = None
_commit = ''


def register(app, root):
  global _marker, _commit
  from openpilot.selfdrive.carrot.server.features import params, setting_profiles
  from openpilot.selfdrive.carrot.server.services import setting_profiles as service
  service.REPO_DIR = str(root)
  setting_profiles.register(app)
  app.router.add_get('/api/param_changes', params.api_param_changes)
  app.router.add_get('/api/param_changes/verify', params.api_param_changes_verify)
  app.router.add_get('/api/param_fingerprint', params.api_param_fingerprint)
  app.router.add_post('/api/param_fingerprint/baseline', params.api_param_fingerprint_baseline)
  real = shutil.which('git')
  assert real is not None
  environment = os.environ.copy()
  environment.update(GIT_AUTHOR_NAME='Owned Fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                     GIT_COMMITTER_NAME='Owned Fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid',
                     GIT_AUTHOR_DATE='2020-01-01T00:00:00+00:00', GIT_COMMITTER_DATE='2020-01-01T00:00:00+00:00')
  (root / 'marker.txt').write_text('owned Git fixture\n')
  for arguments in (['init', '-q'], ['add', 'marker.txt'], ['-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'owned fixture'],
                    ['remote', 'add', 'origin', 'https://github.com/owned/fixture.git']):
    subprocess.run([real, *arguments], cwd=root, env=environment, check=True, capture_output=True)
  _commit = subprocess.check_output([real, 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
  _marker = root / 'git-calls.jsonl'
  directory = root / 'git-bin'
  directory.mkdir()
  wrapper = directory / 'git'
  wrapper.write_text('#!/usr/bin/python3\nimport json,os,sys\n'
                     f'with open({str(_marker)!r}, "a") as stream: stream.write(json.dumps(sys.argv[1:])+"\\n")\n'
                     f'os.execv({real!r}, [{real!r}, *sys.argv[1:]])\n')
  wrapper.chmod(0o755)
  os.environ['PATH'] = str(directory) + os.pathsep + os.environ['PATH']


def cases():
  route = '/api/setting_profiles'
  return {
    'profiles-empty': ('GET', route, None),
    'profiles-malformed': ('POST', route, b'{bad'),
    'profiles-non-object': ('POST', route, []),
    'profiles-limit': ('POST', route, {'name': 'rejected full store'}),
    'profiles-create': ('POST', route, {'name': '  owned\n profile  '}),
    'profiles-read': ('GET', route, None),
    'profiles-head': ('HEAD', route, None),
    'profiles-missing-id': ('POST', route + '/update', {}),
    'profiles-missing': ('POST', route + '/delete', {'id': 'not-found'}),
    'profiles-no-values': ('POST', route + '/update', {'id': '$created', 'values': {}}),
    'profiles-update': ('POST', route + '/update', {'id': '$created', 'name': 'updated', 'values': {'FutureSetting': 21, 'HiddenSetting': 9, 'not-catalog': 5}}),
    'profiles-preview': ('POST', route + '/preview', {'id': '$created'}),
    'profiles-empty-override': ('POST', route + '/apply', {'id': '$created', 'values': {}}),
    'profiles-apply': ('POST', route + '/apply', {'id': '$created'}),
    'history-read': ('GET', '/api/param_changes', None),
    'history-filter': ('GET', '/api/param_changes?limit=bad&name=FutureSetting&source=profile', None),
    'history-verify': ('GET', '/api/param_changes/verify', None),
    'fingerprint-first': ('GET', '/api/param_fingerprint', None),
    'fingerprint-baseline': ('POST', '/api/param_fingerprint/baseline', {}),
    'fingerprint-repeat': ('GET', '/api/param_fingerprint', None),
    'profiles-delete': ('POST', route + '/delete', {'id': '$created'}),
    'profiles-empty-final': ('GET', route, None),
  }


def prepare(scenario, root):
  if scenario not in ('profiles-limit', 'profiles-create'):
    return
  profiles = []
  if scenario == 'profiles-limit':
    profiles = [{'id': f'seed-{index}', 'name': f'seed {index}', 'created_at': '', 'updated_at': '',
                 'meta': {}, 'values': {'CruiseGapLevels': 3}} for index in range(40)]
  for directory in ('data', 'native_data'):
    path = root / directory / 'state/setting_profiles.json'
    path.write_text(json.dumps({'profiles': profiles}))


def payload(side, body):
  if isinstance(body, dict) and body.get('id') == '$created':
    return {**body, 'id': _ids[side]}
  return body


def marker_count():
  return len(_marker.read_text().splitlines()) if _marker is not None and _marker.exists() else 0


def capture(scenario, side, row, before, output):
  calls = marker_count() - before
  if scenario in ('profiles-malformed', 'profiles-non-object', 'profiles-limit'):
    assert calls == 0, (scenario, side, calls)
  if scenario == 'profiles-create':
    body = json.loads(bytes.fromhex(row['body_hex']))
    profile = body['profile']
    assert re.fullmatch('[0-9a-f]{32}', profile['id']), profile
    assert profile['meta']['commit'] == _commit, profile
    assert profile['meta']['commit_url'] == f'https://github.com/owned/fixture/commit/{_commit}'
    instant = datetime.fromisoformat(profile['created_at'])
    assert instant.utcoffset().total_seconds() == 0 and abs((datetime.now(timezone.utc)-instant).total_seconds()) < 30
    assert calls == 4, (side, calls)
    _ids[side] = profile['id']
  if scenario.startswith('profiles-'):
    (output / f'{scenario}-{side}.git.json').write_text(json.dumps({'calls': calls, 'marker': _marker.read_text() if _marker.exists() else ''}))


def normalize(side, row):
  def value(data):
    if isinstance(data, list):
      return [value(item) for item in data]
    if isinstance(data, dict):
      result = {key: value(item) for key, item in data.items()}
      if data.get('id') == _ids.get(side) and isinstance(data.get('values'), dict):
        result['id'] = '$created'
        for key in ('created_at', 'updated_at'):
          instant = datetime.fromisoformat(data[key])
          assert instant.utcoffset().total_seconds() == 0
          result[key] = '$captured-utc'
      return result
    return data
  result = dict(row)
  if row['scenario'].startswith('profiles-'):
    raw = bytes.fromhex(row['body_hex'])
    if raw.startswith(b'{'):
      result['body_hex'] = json.dumps(value(json.loads(raw))).encode().hex()
  if 'state' in row:
    saved = dict(row['state'])
    for name in ('setting_profiles.json', 'setting_profiles.json.tmp'):
      if saved.get(name):
        saved[name] = json.dumps(value(json.loads(bytes.fromhex(saved[name])))).encode().hex()
    result['state'] = saved
  return result

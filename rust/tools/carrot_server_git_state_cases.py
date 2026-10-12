from __future__ import annotations

import json


def cases() -> list[dict]:
  clock = dict(seconds=1000.75, nanoseconds=1000750000000)
  rows = [
    dict(name='missing', steps=[dict(operation='read'), dict(operation='meta', name='GitPullTime'), dict(operation='auto_read')]),
    dict(name='directory_creation', state_missing=True, steps=[dict(operation='read'), dict(operation='write', data={'created': True})]),
    dict(name='malformed', raw=b'{broken', steps=[dict(operation='read')]),
    dict(name='invalid_utf8', raw=b'{"a":"\xff"}', steps=[dict(operation='read')]),
    dict(name='nonobject', raw=b'[1,2]', steps=[dict(operation='read'), dict(operation='auto_read')]),
    dict(name='duplicate_keys', raw=b'{"git_pull_time":1,"git_pull_time":2}', steps=[dict(operation='read'), dict(operation='meta', name='GitPullTime')]),
    dict(name='read_directory', directory_file=True, steps=[dict(operation='read'), dict(operation='write', data={'x': 1})]),
    dict(name='state_directory_is_file', state_file=True, steps=[dict(operation='read'), dict(operation='write', data={'x': 1})]),
    dict(name='replace_failure', directory_file=True, steps=[dict(operation='event', status='failed', fields={'error': 'kept'}, **clock)]),
    dict(name='replace_recovery', directory_file=True, steps=[dict(operation='write', data={'old': 1}), dict(fixture_action='repair_target'), dict(operation='write', data={'new': 2}), dict(operation='read')]),
    dict(name='compact_unicode', steps=[dict(operation='write', data={'z': ' 한글 😀 \ud800\x7f\n"\\', 'a': [1.5, True, None, {'x': ' spaced '}]})]),
    dict(name='nonfinite', steps=[dict(operation='write', data={'nan': float('nan'), 'p': float('inf'), 'n': -float('inf')}), dict(operation='read')]),
    dict(name='scalar_write', steps=[dict(operation='write', data=17), dict(operation='read')]),
    dict(name='duplicate_status_binding', raw=b'{"auto_update":{"status":"old"}}', steps=[dict(operation='event', status='updated', fields={'status': 'forged'}, **clock), dict(operation='auto_read')]),
    dict(name='event_merge', raw=json.dumps({'keep': 3, 'auto_update': {'status': 'old', 'old_field': 'preserved', 'error': 'old'}, 'auto_update_history': ['legacy']}).encode(), steps=[dict(operation='event', status='updated', fields={'event_id': 'forged', 'updated_at': -1, 'new_head': 'abc', 'other': 4}, **clock), dict(operation='auto_read')]),
    dict(name='history_twenty', raw=json.dumps({'auto_update_history': list(range(23))}).encode(), steps=[dict(operation='event', status='', fields={'reset_rc': 2}, **clock)]),
    dict(name='history_wrong_type', raw=b'{"auto_update":[],"auto_update_history":{}}', steps=[dict(operation='event', status=False, fields={'attempted_at': 9}, **clock)]),
    dict(name='project_history_fields', steps=[dict(operation='event', status='pulling', fields={key: index for index, key in enumerate(['attempted_at', 'old_head', 'new_head', 'target_head', 'reset_rc', 'pull_rc', 'error_code', 'error', 'reboot_mode', 'reboot_requested_head', 'omit'])}, **clock)]),
  ]
  for index, value in enumerate([None, True, False, 12, -3.75, '  +1_024  ', '１２３', '', [], {}, float('nan'), float('inf')]):
    rows.append(dict(name=f'pull_time_{index}', steps=[dict(operation='pull_time', timestamp=value, **clock), dict(operation='read')]))
  for index, value in enumerate([None, False, 1.25, ' \x1c 12 \x1f ', ['x', True], {'x': '한글'}, '\ud800']):
    rows.append(dict(name=f'meta_{index}', raw=json.dumps({'git_pull_time': value}).encode(), steps=[dict(operation='meta', name='GitPullTime'), dict(operation='meta', name='other')]))
  outputs = [None, False, '', ' Already up to date. ', 'Already up-to-date Fast-forward', 'Fast-forward', 'Merge made by the recursive strategy', 'Updating abcd..efgh', '1 file changed', '234 files changed, 1 insertion(+)', '1\u00a0file\u2028changed', '1\x1cfile\x1fchanged', '١ file changed', '1 file changedagain', 'files changed', '1 fileschanged', '\ud800FAST-FORWARD', ['fast-forward'], {'x': 'already up to date fast-forward'}]
  rows.append(dict(name='pull_output', steps=[dict(operation='did_pull', output=value) for value in outputs]))
  return rows

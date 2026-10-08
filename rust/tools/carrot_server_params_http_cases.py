def cases():
  bulk = '/api/params_bulk'
  put = '/api/param_set'
  return {
    'bulk-missing': ('GET', bulk, None),
    'bulk-commas': ('GET', bulk + '?names=,,', None),
    'bulk-read': ('GET', bulk + '?names=IsMetric,CruiseGapLevels,FutureSetting,Unknown,DeviceType,IsMetric', None),
    'bulk-hidden': ('GET', bulk + '?names=HiddenSetting', None),
    'bulk-hidden-malformed': ('GET', bulk + '?names=HiddenSetting', None),
    'bulk-head': ('HEAD', bulk + '?names=IsMetric', None),
    'bulk-method': ('POST', bulk, {}),
    'set-malformed': ('POST', put, b'{bad'),
    'set-non-object': ('POST', put, []),
    'set-missing': ('POST', put, {'value': 1}),
    'set-round': ('POST', put, {'name': 'FutureSetting', 'value': 14.999, 'source': 'web_ui'}),
    'set-lower': ('POST', put, {'name': 'FutureSetting', 'value': -100, 'source': 'intro'}),
    'set-upper': ('POST', put, {'name': 'FutureSetting', 'value': 100, 'source': 'invented'}),
    'set-bool': ('POST', put, {'name': 'IsMetric', 'value': 'true', 'source': 'web_ui'}),
    'set-float': ('POST', put, {'name': 'UptimeOnroad', 'value': 0.1, 'source': 'web_ui'}),
    'set-known-write-io': ('POST', put, {'name': 'DisableDM', 'value': 2, 'source': 'web_ui'}),
    'set-unknown': ('POST', put, {'name': 'UnknownWithoutDefinition', 'value': 1}),
    'set-time': ('POST', put, {'name': 'InstallDate', 'value': '2026-10-08'}),
    'set-invalid': ('POST', put, {'name': 'FutureSetting', 'value': 'invalid'}),
    'set-method': ('GET', put, None),
    'set-put': ('PUT', put, {}),
    'bulk-saved': ('GET', bulk + '?names=FutureSetting,IsMetric,UptimeOnroad,CruiseGapLevels', None),
    'bulk-changed': ('GET', bulk + '?names=FutureSetting', None),
    'bulk-repeat': ('GET', bulk + '?names=FutureSetting', None),
  }


def prepare(scenario, root, params, definition):
  if scenario == 'bulk-missing':
    definition['params'].append({'name': 'FutureSetting', 'group': 'Fixture', 'min': 0, 'max': 60, 'default': 20})
    definition['params'].append({'name': 'HiddenSetting', 'group': 'Fixture', 'min': 0, 'max': 60, 'default': 7, 'hidden_brands': ['hyundai']})
    for store in params:
      store.put('CarName', 'HYUNDAI FIXTURE')
    import json
    import os
    settings = root / 'settings.json'
    settings.write_text(json.dumps(definition))
    os.utime(settings, (1700000300, 1700000300))
  if scenario == 'bulk-changed':
    from pathlib import Path
    for store in params:
      Path(store.get_param_path('FutureSetting')).write_bytes(b'19')
  if scenario == 'bulk-hidden-malformed':
    import json
    import os
    definition['params'][-1]['hidden_brands'] = 42
    settings = root / 'settings.json'
    settings.write_text(json.dumps(definition))
    os.utime(settings, (1700000301, 1700000301))
  if scenario == 'set-known-write-io':
    from pathlib import Path
    for store in params:
      Path(store.get_param_path('DisableDM')).mkdir()

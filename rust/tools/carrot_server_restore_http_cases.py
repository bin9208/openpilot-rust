import json
import os
from pathlib import Path


def register(app, root):
  from openpilot.selfdrive.carrot.server.features import params, cars
  (root / 'cars').mkdir()
  cars.SUPPORTED_CAR_GLOB = str(root / 'cars/SupportedCars*')
  cars.register(app)
  app.router.add_get('/api/params_qr_backup', params.api_params_qr_backup)
  app.router.add_post('/api/params_restore_preview', params.api_params_restore_preview)
  app.router.add_post('/api/params_restore_json', params.api_params_restore_json)
  params.PARAMS_BACKUP_PATH = str(root / 'params_backup.json')
  app.router.add_get('/download/params_backup.json', params.handle_download_params_backup)


def cases():
  from openpilot.selfdrive.carrot.server.services import params
  preview, apply = '/api/params_restore_preview', '/api/params_restore_json'
  values = {'IsMetric': 'yes', 'FutureSetting': '21', 'CruiseGapLevels': 4,
            'CarName': 'RESTORED FIXTURE', 'InstallDate': '2026-10-08',
            'CarParams': 'unsupported', 'UnknownWithoutDefinition': 9}
  encoded = params.build_params_qr_payload({'IsMetric': True, 'CruiseGapLevels': 3})['payload']
  return {
    'cars-empty': ('GET', '/api/cars', None),
    'cars-files': ('GET', '/api/cars', None),
    'cars-head': ('HEAD', '/api/cars', None),
    'cars-method': ('POST', '/api/cars', {}),
    'qr-backup': ('GET', '/api/params_qr_backup', None),
    'qr-backup-head': ('HEAD', '/api/params_qr_backup', None),
    'qr-backup-method': ('POST', '/api/params_qr_backup', {}),
    'restore-missing': ('POST', preview, {}),
    'restore-malformed': ('POST', preview, b'{bad'),
    'restore-null': ('POST', preview, None),
    'restore-list': ('POST', preview, []),
    'restore-utf8': ('POST', preview, b'{"payload":"\xff"}'),
    'restore-utf8-short': ('POST', preview, b'{"payload":"\xe2\x82'),
    'restore-json-string': ('POST', preview, {'payload': json.dumps({'FutureSetting': 21})}),
    'restore-cqr': ('POST', preview, {'payload': encoded}),
    'restore-bad-cqr': ('POST', preview, {'payload': 'CQR3:!!!'}),
    'restore-dict': ('POST', preview, {'values': values}),
    'restore-empty-values': ('POST', preview, {'values': {}, 'payload': 'invalid'}),
    'restore-selected': ('POST', preview, {'values': values, 'keys': ['IsMetric', 'FutureSetting']}),
    'restore-non-list-keys': ('POST', preview, {'values': values, 'keys': 'IsMetric'}),
    'restore-invalid-keys': ('POST', preview, {'values': values, 'keys': [[]]}),
    'restore-get': ('GET', preview, None),
    'restore-method': ('PUT', preview, {}),
    'restore-apply': ('POST', apply, {'values': values, 'keys': ['IsMetric', 'FutureSetting']}),
    'restore-repeat': ('POST', apply, {'values': values, 'keys': ['IsMetric', 'FutureSetting']}),
    'restore-invalid': ('POST', apply, {'values': {'CruiseGapLevels': 'bad', 'UptimeOnroad': 'bad'}}),
    'restore-history': ('GET', '/api/param_changes', None),
    'download-missing': ('GET', '/download/params_backup.json', None),
    'download-file': ('GET', '/download/params_backup.json', None),
    'download-head': ('HEAD', '/download/params_backup.json', None),
    'download-range': ('GET', '/download/params_backup.json', None),
    'download-cached': ('GET', '/download/params_backup.json', None),
    'download-directory': ('GET', '/download/params_backup.json', None),
    'download-fifo': ('GET', '/download/params_backup.json', None),
    'download-method': ('POST', '/download/params_backup.json', {}),
    'restore-catalog-missing': ('POST', preview, {'values': {'FutureSetting': 1, 'IsMetric': False}}),
    'restore-catalog-corrupt': ('POST', preview, {'values': {'FutureSetting': 1, 'IsMetric': False}}),
  }


def unavailable_cases():
  return {
    'qr-unavailable': ('GET', '/api/params_qr_backup', None),
    'qr-unavailable-head': ('HEAD', '/api/params_qr_backup', None),
    'restore-unavailable': ('POST', '/api/params_restore_preview', b'bad json'),
    'restore-unavailable-apply': ('POST', '/api/params_restore_json', {}),
  }


def select(family, scenarios, preferences):
  if family == 'restore':
    return (), cases()
  if family == 'restore-unavailable':
    return (), unavailable_cases()
  return scenarios, preferences


def prepare(scenario, root, stores, definition):
  if scenario == 'cars-files':
    (root / 'cars/SupportedCarsFixture').write_bytes(b'FIXTURE Alpha\nFIXTURE Beta\nFIXTURE Alpha\n\xffMAKER Unicode\n')
  if scenario == 'cars-empty':
    definition['params'].append({'name': 'FutureSetting', 'group': 'Fixture', 'min': 0, 'max': 60, 'default': 20})
    (root / 'settings.json').write_text(json.dumps(definition))
    os.utime(root / 'settings.json', (1700000300, 1700000300))
    for store in stores:
      store.put_bool('IsMetric', False)
      store.put_int('CruiseGapLevels', 3)
  if scenario == 'restore-catalog-missing':
    (root / 'settings.json').unlink()
  if scenario == 'restore-catalog-corrupt':
    (root / 'settings.json').write_text('{bad')
    os.utime(root / 'settings.json', (1700000400, 1700000400))
  if scenario == 'download-file':
    (root / 'params_backup.json').write_bytes(b'{"IsMetric": true, "CruiseGapLevels": 3}\n')
    os.utime(root / 'params_backup.json', (1700000500, 1700000500))
  if scenario == 'download-directory':
    (root / 'params_backup.json').unlink()
    (root / 'params_backup.json').mkdir()
  if scenario == 'download-fifo':
    (root / 'params_backup.json').rmdir()
    os.mkfifo(root / 'params_backup.json')


def headers(scenario, root):
  if scenario == 'download-range':
    return {'Range': 'bytes=0-9'}
  if scenario == 'download-cached':
    stat = (root / 'params_backup.json').stat()
    return {'If-None-Match': f'"{stat.st_mtime_ns:x}-{stat.st_size:x}"'}
  return {}


def capture(scenario, store, row, output, index, side):
  if not scenario.startswith(('restore-', 'qr-')):
    return
  saved = {name: Path(store.get_param_path(name)).read_bytes().hex()
           if Path(store.get_param_path(name)).is_file() else None
           for name in ('IsMetric', 'CruiseGapLevels', 'FutureSetting', 'CarName', 'InstallDate', 'UptimeOnroad')}
  (output / f'{index}-{side}.params.json').write_text(json.dumps(saved))
  row['params'] = saved

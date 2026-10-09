def register(app, root, params):
  from openpilot.selfdrive.carrot.server.features import static
  from openpilot.selfdrive.carrot.server.features.intro import state
  from openpilot.selfdrive.carrot.server.features.intro import routes
  static.WEB_DIR = str(root / 'web')
  static._LANGUAGES_JSON_PATH = str(root / 'openpilot/selfdrive/ui/translations/languages.json')
  state.Params = lambda: params
  app.router.add_get('/', static.handle_index)
  routes.register(app)


def cases():
  return {
    'index-missing-existing-install': ('GET', '/', None),
    'index-real-bootstrap-ready': ('GET', '/', None),
    'index-real-bootstrap-repeat': ('GET', '/', None),
    'intro-state-composed': ('GET', '/api/intro/state', None),
    'intro-complete-composed': ('POST', '/api/intro/complete', {'reason': 'user_finished'}),
    'index-real-bootstrap-completed': ('GET', '/', None),
    'index-real-bootstrap-fresh': ('GET', '/', None),
    'index-real-bootstrap-head': ('HEAD', '/', None),
  }


def prepare(scenario, root, params):
  if scenario == 'index-real-bootstrap-ready':
    import hashlib
    import json
    (root / 'web/index.html').write_text('<!doctype html><html><head><script id="carrotAssetManifest" type="application/json"></script><script src="/js/app.js"></script></head><body>owned index</body></html>')
    manifest = root / 'web/generated/asset-manifest.json'
    manifest.parent.mkdir(parents=True)
    manifest.write_text(json.dumps({'schemaVersion': 1, 'assets': [{'id': 'app.runtime', 'kind': 'bundle', 'source': 'src/app.js', 'path': 'js/app.js', 'hash': hashlib.sha256((root / 'web/js/app.js').read_bytes()).hexdigest()}]}))
    languages = root / 'openpilot/selfdrive/ui/translations/languages.json'
    languages.parent.mkdir(parents=True)
    languages.write_text(json.dumps({'English': 'main_en', '한국어': 'main_ko'}, ensure_ascii=False))
    for store in params:
      store.put('LanguageSetting', 'main_en')
  if scenario == 'index-real-bootstrap-fresh':
    from pathlib import Path
    for store in params:
      store.put('CarSelected3', 'MOCK')
      store.put('CruiseGapLevels', store.get_default_value('CruiseGapLevels'))
      Path(store.get_param_path('CarName')).unlink(missing_ok=True)
    for directory in ('data', 'native_data'):
      (root / directory / 'state/intro.json').unlink(missing_ok=True)
  if scenario != 'index-missing-existing-install':
    return
  for store in params:
    store.put('CarSelected3', 'HYUNDAI SONATA')
  for directory in ('data', 'native_data'):
    for name in ('intro.json', 'web_settings.json', 'setting_profiles.json', 'setting_favorites.json', 'youtube_live.json'):
      (root / directory / 'state' / name).unlink(missing_ok=True)

# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with the existing oracle Python/PYTHONPATH; no dependency installation.
# python rust/tools/carrot_server_web_settings.py --binary rust/target/debug/examples/web_settings --output .omo/evidence/225-web-settings
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
from threading import Thread
from typing import Iterator, Literal, TypeAlias, TypedDict, assert_never

from openpilot.selfdrive.carrot.server.services import web_capabilities as caps
from openpilot.selfdrive.carrot.server.services import web_settings as source

Json: TypeAlias = None | bool | int | float | str | list["Json"] | dict[str, "Json"]
Action: TypeAlias = Literal['defaults', 'spec', 'validate', 'layout', 'sanitize', 'capability_spec', 'capability_defaults', 'known_capability', 'capabilities', 'read', 'write', 'update', 'set_capability', 'load', 'clear']
Fixture: TypeAlias = Literal['', 'missing', 'invalid', 'array', 'old', 'parent_file', 'destination_dir', 'catalog_invalid', 'catalog_missing', 'catalog_good', 'catalog_same_signature', 'catalog_race_once', 'catalog_race_twice']


class Case(TypedDict, total=False):
  action: Action
  data: Json
  catalog: Json
  area_1: Json
  area_2: Json
  source: str
  id: str
  enabled: bool
  settings_path: str
  catalog_path: str
  fixture: Fixture


def cases(catalog: Json) -> Iterator[Case]:
  yield {'action': 'defaults'}
  yield {'action': 'spec', 'catalog': catalog}
  yield {'action': 'capability_spec'}
  for capability in ('web_lab', '', 'missing'):
    yield {'action': 'known_capability', 'id': capability}
    yield {'action': 'capability_defaults', 'id': capability}
  coercion_values: list[Json] = [None, False, True, 0, 1, -2, [], [1], {}, {'a': 1}, '', ' TRUE ', ' off ', 'invalid', '\x1cen_US\x1f']
  for field in source.WEB_SETTINGS_SPEC:
    for value in coercion_values:
      yield {'action': 'sanitize', 'data': {field.key: value}, 'catalog': catalog}
    for choice in sorted(field.choices or ()):
      yield {'action': 'sanitize', 'data': {field.key: choice.upper()}, 'catalog': catalog}
  for raw in (None, [], 2, 'bad', {}, {'toss_upload_url': 'https://example.invalid///'}, {'toss_upload_url': 'https://old.invalid', 'web_upload_url': ''}):
    yield {'action': 'sanitize', 'data': raw, 'catalog': catalog}
  for value in ('main_ko', 'main_en', 'main_zh-CHS', 'main_zh-cht', 'ko-KR', 'zh_Hant', 'enGB', 'ja', '\ud800'):
    yield {'action': 'sanitize', 'data': {'web_language': value}, 'catalog': catalog}
  for value in (' https://shind0.synology.me/// ', 'https://op.wjcloud.kr', 'https://\u017fhind0.synology.me', 'HTTPS://example.invalid', 'http://example.invalid/a///', False, 'https://example.invalid/\ud800'):
    yield {'action': 'sanitize', 'data': {'web_upload_url': value}, 'catalog': catalog}
  for value in (-100, 0.2999, 0.3, 0.325, 0.375, 0.425, 0.475, 0.525, 0.575, 0.625, 0.675, 0.7, 100, float('nan'), float('inf'), float('-inf'), ' 0.475 ', 'bad', 10**400):
    yield {'action': 'sanitize', 'data': {'carrot_navi_split_ratio': value, 'carrot_navi_vertical_split_ratio': value}, 'catalog': catalog}
  for first in ('vision', 'navigation', 'drive_insights', 'missing', None):
    for second in ('vision', 'navigation', 'drive_insights', 'missing', None):
      for mode in ('live', 'replay', 'missing'):
        yield {'action': 'layout', 'catalog': catalog, 'area_1': first, 'area_2': second, 'source': mode}
  invalid: list[Json] = [None, [], {}, {'schemaVersion': 1}, {'schemaVersion': 1, 'defaults': {}, 'contents': []}]
  for key, values in [('schemaVersion', [True, 1.0, 2]), ('defaults', [None, [], {}, {'primary': 'missing', 'secondary': 'vision'}]), ('contents', [None, [], [None], [{}]])]:
    for value in values:
      changed = deepcopy(catalog)
      changed[key] = value
      invalid.append(changed)
  for key, values in [('id', ['', '1bad', 'UPPER', 'x-y', None]), ('labelKey', ['', ' \t\x1c', None]), ('singleton', [1, None]), ('supportedSlots', [[], ['primary', 'primary'], ['bad'], [1]]), ('supportedSources', [[], ['live', 'live'], ['bad'], None])]:
    for value in values:
      changed = deepcopy(catalog)
      changed['contents'][0][key] = value
      invalid.append(changed)
  changed = deepcopy(catalog)
  changed['contents'].append(deepcopy(changed['contents'][0]))
  invalid.append(changed)
  for key in ('root', 'defaults', 'descriptor'):
    changed = deepcopy(catalog)
    target = {'root': changed, 'defaults': changed['defaults'], 'descriptor': changed['contents'][0]}[key]
    target['unknown'] = True
    invalid.append(changed)
  for data in [catalog, *invalid]:
    yield {'action': 'validate', 'data': data}
  for singleton in (False, True):
    changed = deepcopy(catalog)
    changed['contents'] = [changed['contents'][0]]
    changed['contents'][0]['singleton'] = singleton
    changed['defaults']['secondary'] = 'vision'
    yield {'action': 'layout', 'catalog': changed, 'area_1': 'vision', 'area_2': 'vision'}
  for sources, slots in [(['replay'], ['primary', 'secondary']), (['live'], ['secondary']), (['live'], ['primary'])]:
    changed = deepcopy(catalog)
    for descriptor in changed['contents']:
      descriptor['supportedSources'] = sources
      descriptor['supportedSlots'] = slots
    yield {'action': 'layout', 'catalog': changed, 'area_1': 'vision', 'area_2': 'navigation'}
  for value in coercion_values:
    yield {'action': 'capabilities', 'data': {'web_lab_enabled': value}}
  yield from [
    {'action': 'read', 'fixture': 'missing'}, {'action': 'read', 'fixture': 'invalid'},
    {'action': 'read', 'fixture': 'array'}, {'action': 'read', 'fixture': 'old'},
    {'action': 'update', 'data': {'start_page': 'TOOLS', 'unknown': True}},
    {'action': 'write', 'data': {'kmap_url': ' https://example.invalid/한국\n길 '}, 'fixture': 'missing'},
    {'action': 'read'}, {'action': 'update', 'data': {'vision_ar_enabled': True, 'vision_ar_debug': True}},
    {'action': 'set_capability', 'id': 'web_lab', 'enabled': True},
    {'action': 'update', 'data': {'vision_ar_enabled': True, 'vision_ar_debug': True}},
    {'action': 'set_capability', 'id': 'web_lab', 'enabled': True},
    {'action': 'set_capability', 'id': 'web_lab', 'enabled': False},
    {'action': 'set_capability', 'id': 'missing', 'enabled': True},
    {'action': 'update', 'data': []}, {'action': 'update', 'data': [1]},
    {'action': 'write', 'data': {'kmap_url': '\ud800\ud801'}},
    {'action': 'read'}, {'action': 'write', 'data': {}, 'fixture': 'parent_file'},
    {'action': 'write', 'data': {}, 'fixture': 'destination_dir'},
    {'action': 'read', 'fixture': 'missing'},
    {'action': 'clear'}, {'action': 'load', 'fixture': 'catalog_invalid'},
    {'action': 'load', 'fixture': 'catalog_good'},
    {'action': 'load', 'fixture': 'catalog_same_signature'},
    {'action': 'clear'}, {'action': 'load'},
    {'action': 'load', 'fixture': 'catalog_missing'},
    {'action': 'load', 'fixture': 'catalog_good'},
    {'action': 'clear'}, {'action': 'load', 'fixture': 'catalog_race_once'}, {'action': 'load'},
    {'action': 'clear'}, {'action': 'load', 'fixture': 'catalog_race_twice'}, {'action': 'load'},
  ]


def apply_source(case: Case) -> Json:
  match case['action']:
    case 'defaults': return dict(source.DEFAULT_WEB_SETTINGS)
    case 'spec': return source.web_settings_client_spec()
    case 'validate': return source.validate_drive_content_catalog(case['data'])
    case 'layout': return list(source.normalize_drive_layout_contents(case.get('area_1'), case.get('area_2'), case['catalog'], case.get('source', 'live')))
    case 'sanitize': return source.sanitize_web_settings(case.get('data'))
    case 'capability_spec': return caps.web_capability_client_spec()
    case 'capability_defaults': return source.web_setting_defaults_for_capability(case['id'])
    case 'known_capability': return caps.is_known_web_capability(case['id'])
    case 'capabilities': return caps.resolve_web_capabilities(case['data'])
    case 'read': return source.read_web_settings()
    case 'write': return source.write_web_settings(case['data'])
    case 'update': return source.update_web_settings(case['data'])
    case 'set_capability': return caps.set_web_capability_enabled(case['id'], case['enabled'])
    case 'load': return source.load_drive_content_catalog()
    case 'clear': source._clear_drive_content_catalog_cache(source.DRIVE_CONTENT_CATALOG_PATH); return None
    case unknown: assert_never(unknown)


def race_catalog(root: Path, attempts: int, catalog: Json) -> None:
  path = root / 'catalog.json'
  for attempt in range(attempts):
    with path.open('w') as writer:
      writer.write(json.dumps(catalog)); writer.flush()
      replacement = root / 'catalog-replacement'
      if attempt + 1 < attempts: os.mkfifo(replacement)
      else: replacement.write_text(json.dumps(catalog))
      os.utime(replacement, ns=(6000000000 + attempt, 6000000000 + attempt))
      os.replace(replacement, path)


def prepare(root: Path, fixture: Fixture, catalog: Json) -> Thread | None:
  settings = root / 'state/web_settings.json'
  path = root / 'catalog.json'
  match fixture:
    case 'missing' | 'parent_file' | 'destination_dir':
      if settings.is_dir(): settings.rmdir()
      elif settings.exists(): settings.unlink()
      temporary = settings.with_suffix('.json.tmp')
      if temporary.exists(): temporary.unlink()
      if settings.parent.is_file(): settings.parent.unlink()
      if fixture == 'parent_file': settings.parent.rmdir(); settings.parent.write_text('file')
      else:
        settings.parent.mkdir(exist_ok=True)
        if fixture == 'destination_dir': settings.mkdir()
    case 'invalid' | 'array' | 'old': settings.write_text({'invalid': '{', 'array': '[]', 'old': '{}'}[fixture])
    case 'catalog_invalid': path.write_text('{')
    case 'catalog_missing': path.unlink()
    case 'catalog_good': path.write_text(json.dumps(catalog)); os.utime(path, ns=(2000000000, 2000000000))
    case 'catalog_same_signature': path.write_text(path.read_text().replace('drive_insights', 'other_content')); os.utime(path, ns=(2000000000, 2000000000))
    case 'catalog_race_once' | 'catalog_race_twice':
      path.unlink(); os.mkfifo(path)
      actor = Thread(target=race_catalog, args=(root, 1 if fixture == 'catalog_race_once' else 2, catalog), daemon=True)
      actor.start()
      return actor
    case '': return None
    case unknown: assert_never(unknown)
  return None


def snapshot(path: Path) -> Json:
  return {name: (target.read_bytes().hex() if target.is_file() else ('directory' if target.is_dir() else None))
          for name in ('web_settings.json', 'web_settings.json.tmp') for target in [path / 'state' / name]}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--regressions-only', action='store_true')
  parser.add_argument('--ratio-only', action='store_true')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  catalog = json.loads(Path(source.DRIVE_CONTENT_CATALOG_PATH).read_text())
  roots = [args.output / name for name in ('original-fs', 'native-fs')]
  for root in roots:
    root.mkdir(exist_ok=True)
    (root / 'state').mkdir(exist_ok=True)
    (root / 'catalog.json').write_text(json.dumps(catalog))
  inputs = ([{'action': 'write', 'data': {'kmap_url': '\ud800\ud801'}, 'fixture': 'missing'},
             {'action': 'sanitize', 'data': {'web_upload_url': 'https://\u017fhind0.synology.me'}, 'catalog': catalog}]
            if args.regressions_only else list(cases(catalog)))
  if args.ratio_only:
    inputs = [case for case in inputs if case['action'] == 'sanitize'
              and isinstance(case.get('data'), dict)
              and 'carrot_navi_split_ratio' in case['data']
              and 'carrot_navi_vertical_split_ratio' in case['data']]
  with (args.output / 'inputs.jsonl').open('w') as requests, (args.output / 'original.jsonl').open('w') as expected, (args.output / 'native.jsonl').open('w') as actual, (args.output / 'native.stderr').open('w') as stderr:
    process = subprocess.Popen([str(args.binary.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, stderr=stderr) if args.binary else None
    try:
      for index, case in enumerate(inputs):
        actors = [prepare(root, case.get('fixture', ''), catalog) for root in roots[:2 if process else 1]]
        source.CARROT_WEB_SETTINGS_PATH = str(roots[0] / 'state/web_settings.json')
        source.DRIVE_CONTENT_CATALOG_PATH = str(roots[0] / 'catalog.json')
        try: wanted = apply_source(case)
        except OSError: wanted = {'error': 'filesystem failure'}
        except (ValueError, TypeError, AttributeError, OverflowError) as error: wanted = {'error': str(error)}
        native_case = {**case, 'settings_path': str(roots[1] / 'state/web_settings.json'), 'catalog_path': str(roots[1] / 'catalog.json')}
        request = json.dumps(native_case)
        requests.write(request + '\n')
        record = {'output': wanted, 'files': snapshot(roots[0])}
        expected.write(json.dumps(record) + '\n'); expected.flush()
        if process:
          assert process.stdin is not None and process.stdout is not None
          process.stdin.write(request + '\n'); process.stdin.flush()
          result = {'output': json.loads(process.stdout.readline()), 'files': snapshot(roots[1])}
          actual.write(json.dumps(result) + '\n'); actual.flush()
          assert result == record, f'case {index} mismatch: {case}; see retained original/native outputs'
        for actor in actors:
          if actor: actor.join(timeout=10); assert not actor.is_alive()
    finally:
      if process:
        assert process.stdin is not None
        process.stdin.close()
        assert process.wait(timeout=10) == 0
  receipt = {'cases': len(inputs), 'passed': bool(args.binary), 'comparison': 'Python JSON values, error messages and exact persisted/temp file bytes', 'source_only': not bool(args.binary)}
  (args.output / 'result.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()

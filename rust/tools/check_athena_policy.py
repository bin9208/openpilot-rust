#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
from athena_reference import policy_trace


def scenarios():
  cases = []
  cases.append([{'op': 'strip_zst_extension', 'args': [name]} for name in ['', 'a', 'a.zst', 'a.zst.zst', 'a.ZST']])
  cases.append([{'op': 'setRouteViewed', 'args': [name]} for name in [*map(str, range(14)), '5', '13', '', 'a,b', '한글']])
  for priorities in [[99] * 12, list(range(12)), list(range(12))[::-1], [4, 1, 3, 1, 4, 0, 2, 0, -10, 99, 1, 1]]:
    cases.append([
      {'op': 'uploadFilesToUrls', 'args': [[{'fn': 'file', 'url': f'http://owned.test/{i}?one=1', 'priority': p} for i, p in enumerate(priorities)]]},
      *[{'op': 'pop'} for _ in priorities],
      {'op': 'cache'},
    ])
  cases.append([
    {'op': 'uploadFilesToUrls', 'args': [[{'fn': name, 'url': f'http://owned.test/{i}'} for i, name in enumerate(['', '/absolute', '../up', 'a..b', 'missing', 'file.zst', 'folder', 'folder/child', './file'])]]},
    {'op': 'uploadFileToUrl', 'args': ['file', 'http://owned.test/5?new=token', {}]},
    {'op': 'cancelUpload', 'args': ['unknown']},
  ])
  cases.append([{'op': 'uploadFilesToUrls', 'args': [[{'fn': 'file', 'url': 'https://owned.test/한글', 'headers': {'z': 'one', 'a': "quote'\"", '한글': '\n'}, 'allow_cellular': True}]]}])
  cases.extend([[{'op': 'params', 'values': {'AthenadUploadQueue': invalid}}, {'op': 'initialize'}] for invalid in [None, '', 'bad', {}, [None], [{}]]])
  item = {'path': '/synthetic/file', 'url': 'http://owned.test/current', 'headers': {}, 'created_at': 1720000000125, 'id': 'synthetic-id', 'retry_count': 0, 'current': False, 'progress': 0, 'allow_cellular': False, 'priority': 99}
  cases.append([{'op': 'params', 'values': {'AthenadUploadQueue': [item, {}, item]}}, {'op': 'initialize'}, {'op': 'cache'}])
  cases.append([{'op': 'params', 'values': {'AthenadUploadQueue': [item]}}, {'op': 'initialize'}, {'op': 'cancelFirst'}, {'op': 'cache'}, {'op': 'listUploadQueue'}])
  cases.append([{'op': 'current', 'tid': 1, 'item': dict(item, current=True)}, {'op': 'cancelFirst'}, {'op': 'cache'}])
  for count in [-1, 0, 29, 30, 31]:
    for increase in [False, True]:
      cases.append([{'op': 'current', 'tid': 1, 'item': dict(item, current=True, progress=0.75, retry_count=count)}, {'op': 'retry', 'tid': 1, 'increase': increase}, {'op': 'cache'}])
  return cases


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  records = []
  with tempfile.TemporaryDirectory(prefix='athena-policy-') as temp:
    root = Path(temp)
    (root / 'file').write_text('synthetic')
    (root / 'folder').mkdir()
    (root / 'folder/child').write_text('synthetic')
    cases = scenarios()
    source = [policy_trace(root, case) for case in cases]
    process = subprocess.run([args.binary], input=''.join(json.dumps({'root': str(root), 'operations': case}) + '\n' for case in cases), text=True, capture_output=True, check=True)
    native = [json.loads(line) for line in process.stdout.splitlines()]
    for index, (expected, actual) in enumerate(zip(source, native, strict=True)):
      records.append({'case': index, 'source': expected, 'native': actual, 'pass': expected == actual})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(records, indent=2, ensure_ascii=False) + '\n')
  failures = [record['case'] for record in records if not record['pass']]
  assert not failures, failures
  print(f'PASS: {len(records)} unchanged-source queue/cache/priority/id/path/routes scenarios')


if __name__ == '__main__':
  main()

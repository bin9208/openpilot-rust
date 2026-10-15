"""Temporary original nmcli selection oracle with inert subprocess results."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace


def main():
  parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
  manifest = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  item = manifest['files']['tools/esp32s3_display/provision.py']; data = item['content'].encode()
  assert hashlib.sha256(data).hexdigest() == item['sha256']
  tree = ast.parse(data)
  function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'current_wifi')
  inputs, expected = [], []
  for profiles, password, failure in [('', 'synthetic-password', None), ('a:wifi\nb:802-11-wireless\n', 'synthetic-password', None), ('a:ethernet\nb:wifi\n', 'synthetic-password', None), ('a:802-11-wireless\n', 'synthetic-password', None), ('a:wifi\n', '', None), ('a:wifi\n', 'synthetic-password', 0), ('a:wifi\n', 'synthetic-password', 1), ('a:wifi\n', 'synthetic-password', 2)]:
    calls = []; responses = [profiles, 'synthetic-차량\n', password + '\n']
    def run(command, **kwargs):
      index = len(calls); calls.append(dict(command=command, **kwargs))
      return SimpleNamespace(returncode=int(index == failure), stdout=responses[index])
    scope = dict(subprocess=SimpleNamespace(run=run))
    exec(compile(ast.Module(body=[function], type_ignores=[]), 'e8e152db:provision.py', 'exec'), scope)
    inputs.append(dict(responses=responses, failure=failure))
    try: result = dict(wifi=list(scope['current_wifi']()))
    except RuntimeError as error: result = dict(error=str(error))
    result['calls'] = calls; expected.append(result)
  (args.output / 'current_wifi.json').write_text(json.dumps(dict(source_commit=manifest['source_commit'], input=inputs, expected=expected), indent=2) + '\n')
  print(json.dumps(dict(cases=len(inputs), scope='original current_wifi; inert nmcli outputs/synthetic credentials')))


if __name__ == '__main__': main()

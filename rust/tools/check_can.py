"""Compare every native CAN trace against the unchanged source implementation."""

import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess

from can_cases import codec, databases, ignored, known_fca_rejection, lazy, learning, lifecycle
from can_source import load, trace


def compare(expected, actual, location='root'):
  match expected, actual:
    case dict() as left, dict() as right:
      if 'error' in left:
        if 'error' not in right:
          raise AssertionError(f'{location}: source failure became native success: {left}')
        if left['error'] == 'TypeError' and 'xor_checksum() takes 3 positional' in left['detail']:
          assert 'inherited Volkswagen MLB' in right['error'], (location, left, right)
        elif left['error'] == 'IndexError':
          assert right['error'] == 'invalid checksum payload', (location, left, right)
        elif left['error'] == 'AssertionError':
          assert right['error'].startswith('duplicate CAN message'), (location, left, right)
        elif left['error'] == 'KeyError':
          assert right['error'].startswith('unknown CAN message'), (location, left, right)
        else:
          raise AssertionError(f'{location}: unclassified source/native failure: {left}, {right}')
        return
      assert left.keys() == right.keys(), (location, left.keys(), right.keys())
      for key in left:
        compare(left[key], right[key], f'{location}.{key}')
    case list() as left, list() as right:
      assert len(left) == len(right), (location, len(left), len(right))
      for index, (one, two) in enumerate(zip(left, right, strict=True)):
        compare(one, two, f'{location}[{index}]')
    case float() as left, float() | int() as right:
      assert abs(left - right) <= 1e-12 + abs(left) * 1e-12, (location, left, right)
    case _:
      assert expected == actual, (location, expected, actual)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--staging', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  output = io.StringIO()
  with redirect_stdout(output), redirect_stderr(output):
    paths, identities, missing = databases(args.staging)
    cases = [codec(path) for path in paths.values()]
    lifecycle_cases = [case for path in paths.values() if (case := lifecycle(path)) is not None]
    cases.extend(lifecycle_cases)
    cases.append(learning(paths['test']))
    cases.append(ignored(paths['test']))
    cases.append(known_fca_rejection(paths['fca_giorgio']))
    cases.append(lazy(paths['test']))
    expected = [trace(case) for case in cases]
  request = dict(cases=cases)
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(output.getvalue())
  (args.evidence / 'catalog.json').write_text(json.dumps(dict(identities=identities, paths=paths, missing=missing,
                  sha256={name: hashlib.sha256(Path(path).read_bytes()).hexdigest() for name, path in paths.items()}), indent=2) + '\n')
  actual_path = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), actual_path.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(actual_path.read_text())
  try:
    compare(expected, actual)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  errors = [step for case in expected for step in case['steps'] if 'error' in step]
  missing_results = []
  DBC, _, _ = load()
  for name, path in missing.items():
    try:
      DBC(name)
    except FileNotFoundError as error:
      source_failure = str(error)
    else:
      raise AssertionError(f'source unexpectedly loaded missing DBC {name}')
    probe = dict(cases=[dict(path=path, bus=0, now=0, messages=[], steps=[])])
    failure = subprocess.run([args.binary.resolve(), actual_path.with_name(f'{name}.json').resolve()], input=json.dumps(probe), text=True, capture_output=True)
    assert failure.returncode != 0, (name, failure.stdout, failure.stderr)
    missing_results.append(dict(name=name, source_failure=source_failure, native_exit=failure.returncode, native_stderr=failure.stderr))
  (args.evidence / 'missing-dbcs.json').write_text(json.dumps(missing_results, indent=2) + '\n')
  result = dict(status='available_codec_catalog_matches_source', identities=len(identities), databases=len(paths), missing_source_dbcs=list(missing), cases=len(cases), lifecycle_cases=len(lifecycle_cases),
                messages=sum(len(case['dbc']['messages']) for case in expected[:len(paths)]),
                operations=sum(len(case['steps']) for case in cases), inherited_source_failures=len(errors),
                numeric_tolerance='absolute 1e-12 + relative 1e-12; bytes, states and metadata exact', runtime_python=False)
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

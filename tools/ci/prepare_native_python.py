#!/usr/bin/env python3
"""Actions-only preparation of the pinned Python interpreter, without execution."""
import hashlib
import json
import os
import runpy
from pathlib import Path
import subprocess
import tomllib


def main():
  runpy.run_path(str(Path(__file__).resolve().parent/'runner_policy.py'))['require_environment'](self_hosted_only=False)
  source = Path(os.environ['GITHUB_WORKSPACE']).resolve()
  work = Path(os.environ['CI_JOB_TEMP']).resolve()
  assert (work/'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  assert subprocess.check_output(['git', '-C', source, 'rev-parse', 'HEAD'], text=True).strip() == os.environ['GITHUB_SHA']
  plan = json.loads((source/'rust/native-python-toolchain.json').read_text())
  requires_python = tomllib.loads((source/'pyproject.toml').read_text())['project']['requires-python']
  assert requires_python == '>= 3.12.3, < 3.13'
  assert (3, 12, 3) <= tuple(map(int, plan['version'].split('.'))) < (3, 13, 0)
  preparer = Path((work/'native-capnp-inputs-cli.txt').read_text().strip())
  command = [preparer, '--prepare-python', '--plan', source/'rust/native-python-toolchain.json',
             '--cache', work/'native-python-assets', '--store', work/'native-python-prefixes']
  rows = []
  for phase in ('initial', 'reused'):
    path = work/f'native-python-{phase}.json'
    with path.open('x') as output:
      subprocess.run(list(map(str, command)), stdout=output, check=True)
    row = json.loads(path.read_text())
    assert row['version'] == plan['version'] and row['target'] == plan['target']
    assert row['asset']['sha256'] == plan['asset']['sha256'] and row['asset']['bytes'] == plan['asset']['bytes']
    assert not row['installer_executed'] and not row['interpreter_executed'] and not row['runtime_activated']
    assert (row['copied_bytes'] > 0) == (phase == 'initial')
    if phase == 'reused':
      assert row['copied_bytes'] == row['asset']['transferred_bytes'] == row['asset']['stored_bytes'] == row['expected_additional_bytes'] == 0
    tree_bytes = Path(row['tree_receipt']).read_bytes()
    assert hashlib.sha256(tree_bytes).hexdigest() == row['tree_sha256']
    with Path(row['python']).open('rb') as stream:
      header = stream.read(64)
    assert header[:7] == b'\x7fELF\x02\x01\x01' and int.from_bytes(header[18:20], 'little') == 183
    rows.append(row)
  assert all(rows[0][key] == rows[1][key] for key in ('identity', 'root', 'python', 'tree_sha256'))
  result = dict(schema=1, source_commit=os.environ['GITHUB_SHA'], version=plan['version'],
      project_requires_python=requires_python,
      target=plan['target'], archive_sha256=plan['asset']['sha256'], archive_bytes=plan['asset']['bytes'],
      tree_sha256=rows[0]['tree_sha256'], first_http_body_bytes=rows[0]['asset']['transferred_bytes'],
      repeated_http_body_bytes=0, repeated_copied_bytes=0, interpreter_executed=False,
      python_model_environment_complete=False, model_generated=False, runtime_activated=False,
      physical_device_execution=False)
  (work/'native-python-summary.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

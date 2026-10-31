#!/usr/bin/env python3
"""Actions-only preparation of the two native Python model input wheels."""
import hashlib
import json
import os
import runpy
from pathlib import Path
import subprocess


def digest(path):
  with path.open('rb') as stream: return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
  runpy.run_path(str(Path(__file__).resolve().parent/'runner_policy.py'))['require_environment'](self_hosted_only=False)
  source, work = Path(os.environ['GITHUB_WORKSPACE']), Path(os.environ['CI_JOB_TEMP'])
  assert (work/'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB'))
  assert subprocess.check_output(['git','-C',source,'rev-parse','HEAD'],text=True).strip() == os.environ['GITHUB_SHA']
  preparer = Path((work/'native-capnp-inputs-cli.txt').read_text().strip())
  command = ['/usr/bin/python3',source/'rust/tools/native_model_inputs.py','--root',source,'--preparer',preparer,
      '--cache',work/'native-model-wheel-assets','--store',work/'native-model-wheel-packages',
      '--environment-store',work/'native-model-wheel-environments']
  rows = []
  for phase in ('initial','reused'):
    path = work/f'native-model-wheels-{phase}.json'
    subprocess.run(list(map(str,[*command,'--output',path])),check=True)
    row = json.loads(path.read_text())
    assert row['source']['commit'] == os.environ['GITHUB_SHA']
    assert set(row['packages']) == {'numpy','zstandard'}
    assert (row['copied_bytes'] > 0) == (phase == 'initial')
    assert (row['linked_bytes'] > 0) == (phase == 'initial')
    assert not row['installer_executed'] and not row['wheels_imported'] and not row['compiler_executed']
    assert not row['model_generated'] and not row['runtime_activated']
    assert digest(Path(row['tree_receipt'])) == row['tree_sha256']
    if phase == 'reused':
      assert row['transferred_bytes'] == row['stored_bytes'] == row['copied_bytes'] == row['linked_bytes'] == 0
    rows.append(row)
  assert rows[0]['root'] == rows[1]['root'] and rows[0]['tree_sha256'] == rows[1]['tree_sha256']
  # The Python wheel retains the same existing foreign numerical C libraries.
  numerics = json.loads((source/'rust/native-numerics-provider.json').read_text())
  matches = []
  for name, member in numerics['members'].items():
    if '.so' not in name: continue
    files = list(Path(rows[0]['site_packages']).rglob(Path(name).name))
    assert len(files) == 1 and digest(files[0]) == member['sha256'] and files[0].stat().st_size == member['bytes']
    matches.append(dict(name=Path(name).name,sha256=member['sha256']))
  assert len(matches) == 2
  result = dict(schema=1,source_commit=os.environ['GITHUB_SHA'],tree_sha256=rows[0]['tree_sha256'],
      packages=['numpy','zstandard'],existing_numerics_libraries_match=matches,
      first_http_body_bytes=rows[0]['transferred_bytes'],repeated_http_body_bytes=0,repeated_copied_bytes=0,
      repeated_linked_bytes=0,python_model_environment_executed=False,model_generated=False,
      full_runtime_build=False,physical_device_execution=False)
  (work/'native-model-wheels-summary.json').write_text(json.dumps(result,indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

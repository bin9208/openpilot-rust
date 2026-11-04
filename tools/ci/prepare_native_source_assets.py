#!/usr/bin/env python3
"""Actions-only preparation of the current committed LFS inputs, never firmware."""
import argparse
import hashlib
import json
import os
import runpy
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--work', type=Path, required=True)
  args = parser.parse_args()
  runpy.run_path(str(Path(__file__).resolve().parent/'runner_policy.py'))['require_environment'](self_hosted_only=False)
  root, work = args.root.resolve(), args.work.resolve()
  assert root == Path(os.environ['GITHUB_WORKSPACE']).resolve()
  assert work == Path(os.environ['CI_JOB_TEMP']).resolve()
  sha = subprocess.check_output(['git', '-C', root, 'rev-parse', 'HEAD'], text=True).strip()
  assert sha == os.environ['GITHUB_SHA']
  plan = json.loads((root/'rust/native-source-lfs.json').read_text())
  assert len(plan['assets']) == 4
  for path, asset in plan['assets'].items():
    pointer = subprocess.check_output(['git', '-C', root, 'show', 'HEAD:'+path], text=True)
    assert pointer == f'version https://git-lfs.github.com/spec/v1\noid sha256:{asset["sha256"]}\nsize {asset["bytes"]}\n'
  preparer = Path((work/'native-capnp-inputs-cli.txt').read_text().strip())
  command = [preparer, '--prepare-source-lfs', '--root', root, '--plan', root/'rust/native-source-lfs.json',
             '--cache', work/'native-source-lfs-assets', '--store', work/'native-source-lfs-prefixes',
             '--objects', work/'native-source-lfs-objects']
  rows = []
  for phase in ('initial', 'reused'):
    path = work/f'native-source-lfs-{phase}.json'
    with path.open('x') as output:
      subprocess.run(list(map(str, command)), stdout=output, check=True)
    row = json.loads(path.read_text())
    assert row['source']['commit'] == sha and row['origin_commit'] == plan['origin_commit']
    assert set(row['assets']) == set(plan['assets'])
    assert not row['compiler_executed'] and not row['checkout_modified'] and not row['runtime_activated']
    assert (row['copied_bytes'] > 0) == (phase == 'initial')
    assert (row['linked_bytes'] > 0) == (phase == 'initial')
    if phase == 'reused':
      assert not row['asset_cache_consulted'] and row['expected_additional_bytes'] == 0
    receipt = Path(row['tree_receipt']).read_bytes()
    assert hashlib.sha256(receipt).hexdigest() == row['tree_sha256']
    for relative, asset in plan['assets'].items():
      actual = Path(row['root'])/relative
      assert actual.stat().st_size == asset['bytes']
      with actual.open('rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == asset['sha256']
      if phase == 'reused':
        assert row['assets'][relative]['transferred_bytes'] == row['assets'][relative]['stored_bytes'] == 0
    rows.append(row)
  assert all(rows[0][key] == rows[1][key] for key in ('root', 'identity', 'tree_sha256', 'source'))
  assert not subprocess.check_output(['git', '-C', root, 'status', '--porcelain', '--untracked-files=all'], text=True).strip()
  result = dict(schema=1, source_commit=sha, origin_commit=plan['origin_commit'], source_lfs_objects=4,
                declared_bytes=sum(asset['bytes'] for asset in plan['assets'].values()),
                first_http_body_bytes=sum(asset['transferred_bytes'] for asset in rows[0]['assets'].values()),
                repeated_http_body_bytes=0, repeated_copied_bytes=0, repeated_linked_bytes=0,
                tree_sha256=rows[0]['tree_sha256'], ordinary_git_files_replaced=False,
                generated_firmware_prepared=False, python_model_generation_complete=False,
                full_runtime_build=False, physical_device_execution=False)
  (work/'native-source-lfs-summary.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

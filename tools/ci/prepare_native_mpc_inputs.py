#!/usr/bin/env python3
"""Actions-only export/reuse of the source-bound, original C MPC provider."""
import argparse
import json
import os
import runpy
from pathlib import Path
import shutil
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--preparer', type=Path, required=True)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--work', type=Path, required=True)
  args = parser.parse_args()
  runpy.run_path(str(Path(__file__).resolve().parent/'runner_policy.py'))['require_environment'](self_hosted_only=False)
  source = args.root.resolve()
  assert source == Path(os.environ['GITHUB_WORKSPACE']).resolve()
  commit = subprocess.check_output(['git', '-C', source, 'rev-parse', 'HEAD'], text=True).strip()
  assert commit == os.environ['GITHUB_SHA']
  work = args.work.resolve()
  plan_path = source/'rust/native-mpc-provider.json'
  plan = json.loads(plan_path.read_text())
  payload = sum(row['bytes'] for row in plan['members'].values())
  growth = payload*4+16*1024**2
  free = shutil.disk_usage(work).free
  print(json.dumps(dict(available_bytes=free, expected_additional_bytes=growth)), flush=True)
  if free < growth:
    raise RuntimeError('insufficient capacity for this MPC component operation; other inputs retained')

  def execute(command, phase):
    path = work/f'native-mpc-{phase}.json'
    with path.open('x') as output:
      subprocess.run(list(map(str, command)), stdout=output, check=True)
    return json.loads(path.read_text())

  def binding(row):
    value = row['source_binding']
    assert value['source']['commit'] == commit
    assert value['contract_sha256'] == plan['source_contract_sha256']
    assert value['original_source_commit'] == plan['source_commit'] != commit
    assert value['inputs_match'] and not value['regenerated']
    return value

  exported = execute([args.preparer, '--export-released-provider', '--source-root', source,
      '--plan', plan_path, '--cache', work/'native-provider-origin',
      '--store', work/'native-mpc-origins', '--output', work/'native-mpc-export'], 'export')
  assert exported['origin']['asset']['transferred_bytes'] == exported['origin']['asset']['stored_bytes'] == 0
  assert not exported['published_release'] and not exported['provider_executed']
  binding(exported)
  consume = [args.preparer, '--prepare-released-provider', '--source-root', source,
      '--plan', plan_path, '--descriptor', exported['descriptor'],
      '--descriptor-sha256', exported['descriptor_sha256'], '--cache', work/'native-mpc-components',
      '--store', work/'native-mpc-prefixes', '--offline-archive', exported['archive']]
  rows = []
  for phase in ('initial', 'reused'):
    row = execute(consume, phase)
    assert row['tree_sha256'] == exported['tree_sha256']
    assert row['provider_source_commit'] == plan['source_commit']
    assert row['asset']['transferred_bytes'] == row['asset']['stored_bytes'] == 0
    assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
    assert (row['copied_bytes'] > 0) == (phase == 'initial')
    assert (row['offline_seed_bytes'] > 0) == (phase == 'initial')
    assert binding(row) == binding(exported)
    rows.append(row)
  assert rows[0]['root'] == rows[1]['root']
  assert not subprocess.check_output(['git', '-C', source, 'status', '--porcelain', '--untracked-files=all'], text=True).strip()
  result = dict(schema=1, source_commit=commit, source_binding=binding(rows[0]),
      profile=plan['profile'], tree_sha256=exported['tree_sha256'],
      descriptor_sha256=exported['descriptor_sha256'], selected_original_members=len(plan['members']),
      payload_bytes=payload, component_bytes=exported['component_bytes'],
      repeated_http_body_bytes=0, repeated_copied_bytes=0,
      consumer_origin_archive_downloaded=False, generated_mpc_solvers_reused=True,
      solvers_regenerated=False, provider_executed=False, full_runtime_build=False,
      runtime_activated=False, published_release=False, public_component_download_tested=False,
      physical_device_execution=False)
  (work/'native-mpc-summary.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

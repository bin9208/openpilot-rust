#!/usr/bin/env python3
"""Actions-only export/reuse of independently pinned foreign components."""
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
  exports = work/'native-companion-exports'
  exports.mkdir()
  # Each component owns a leaf; the provider deliberately requires a real parent.
  (work/'native-companion-components').mkdir()
  (work/'native-companion-prefixes').mkdir()
  summaries = {}
  runtime_inputs = dict(files={}, trees={})

  def execute(command, name):
    path = work/f'native-companion-{name}.json'
    with path.open('x') as output:
      subprocess.run(list(map(str, command)), stdout=output, check=True)
    return json.loads(path.read_text())

  for name in ('numerics', 'geos', 'brotli', 'acados', 'sdl', 'turbojpeg', 'ffmpeg-tools', 'freesans'):
    plan_path = source/f'rust/native-{name}-provider.json'
    plan = json.loads(plan_path.read_text())
    payload = sum(row['bytes'] for row in plan['members'].values())
    growth = payload*4+16*1024**2
    free = shutil.disk_usage(work).free
    print(json.dumps(dict(component=name, available_bytes=free, expected_additional_bytes=growth)), flush=True)
    if free < growth:
      raise RuntimeError('insufficient capacity for this component export/reuse; other inputs retained')
    exported = execute([args.preparer, '--export-released-provider', '--plan', plan_path,
        '--cache', work/'native-provider-origin', '--store', work/'native-provider-origins',
        '--output', exports/name], f'{name}-export')
    assert exported['origin']['asset']['transferred_bytes'] == exported['origin']['asset']['stored_bytes'] == 0
    assert not exported['published_release'] and not exported['provider_executed']
    consume = [args.preparer, '--prepare-released-provider', '--plan', plan_path,
        '--descriptor', exported['descriptor'], '--descriptor-sha256', exported['descriptor_sha256'],
        '--cache', work/'native-companion-components'/name, '--store', work/'native-companion-prefixes'/name,
        '--offline-archive', exported['archive']]
    rows = []
    for phase in ('initial', 'reused'):
      row = execute(consume, f'{name}-{phase}')
      assert row['tree_sha256'] == exported['tree_sha256']
      assert row['provider_source_commit'] == plan['source_commit']
      assert row['asset']['transferred_bytes'] == row['asset']['stored_bytes'] == 0
      assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
      assert (row['copied_bytes'] > 0) == (phase == 'initial')
      assert (row['offline_seed_bytes'] > 0) == (phase == 'initial')
      rows.append(row)
    assert rows[0]['root'] == rows[1]['root']
    runtime_inputs['files'][rows[0]['tree_receipt']] = rows[0]['tree_sha256']
    runtime_inputs['trees'][rows[0]['root']] = rows[0]['tree_receipt']
    summaries[name] = dict(profile=plan['profile'], tree_sha256=exported['tree_sha256'],
        descriptor_sha256=exported['descriptor_sha256'], selected_members=len(plan['members']),
        payload_bytes=payload, component_bytes=exported['component_bytes'],
        origin_repeated_http_body_bytes=0, consumer_origin_archive_downloaded=False,
        repeated_http_body_bytes=0, repeated_copied_bytes=0)
  assert len(runtime_inputs['trees']) == 8 and len(runtime_inputs['files']) == 8
  original = json.loads((Path(exported['origin']['root'])/'provenance/original-runtime-manifest.json').read_text())
  expected_libraries = {row['path'] for row in original['files'] if row['path'].startswith('rust/bin/lib/')}
  selected_libraries = set()
  for name in ('ffmpeg', 'raylib', 'opencv', 'numerics', 'geos', 'brotli', 'acados', 'sdl', 'turbojpeg'):
    profile = json.loads((source/f'rust/native-{name}-provider.json').read_text())
    selected_libraries.update(row['source'] for row in profile['members'].values()
                              if row['source'].startswith('rust/bin/lib/'))
  assert selected_libraries == expected_libraries and len(expected_libraries) == 53
  (work/'native-companion-runtime-inputs.json').write_text(json.dumps(runtime_inputs, indent=2)+'\n')
  assert subprocess.check_output(['git', '-C', source, 'rev-parse', 'HEAD'], text=True).strip() == commit
  assert not subprocess.check_output(['git', '-C', source, 'status', '--porcelain', '--untracked-files=all'], text=True).strip()
  result = dict(schema=1, source_commit=commit, components=summaries,
      original_root_library_paths_covered=53,
      foreign_ffmpeg_tools_prepared=True, cluster_font_prepared=True,
      numerics_component_count=1, intended_numerics_runtime_destinations=4,
      compiler_executed=False, provider_executed=False, project_binaries_reused=False,
      project_generated_mpc_solvers_reused=False, runtime_assets_complete=False,
      full_runtime_build=False, published_release=False, public_component_download_tested=False,
      physical_device_execution=False)
  (work/'native-companion-summary.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
"""Actual pinned ARM provider equivalence and matched cold/warm timing."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import time


def digest(path):
  with Path(path).open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
  parser = argparse.ArgumentParser()
  for name in ('preparer', 'assets', 'root', 'work'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_github_native_arm']()
  source = args.root.resolve()
  assert source == Path(os.environ['GITHUB_WORKSPACE']).resolve()
  work = args.work.resolve()
  assert work.is_relative_to(Path(os.environ['CI_JOB_TEMP']).resolve())
  work.mkdir()
  plans = sorted((source/'rust').glob('native-*-provider.json'))
  pins = {path:json.loads(path.read_bytes()) for path in plans}
  assert len(plans) == 15
  summary = dict(schema=1, source_commit=os.environ['GITHUB_SHA'], profiles=15,
      cases={}, comparisons={}, success=False, device_execution=False, published_release=False)

  def execute(command, name):
    started = time.monotonic()
    with (work/(name+'.json')).open('x') as output, (work/(name+'.stderr')).open('x') as errors:
      result = subprocess.run(list(map(str, command)), stdout=output, stderr=errors)
    summary['cases'][name] = dict(seconds=time.monotonic()-started, exit_code=result.returncode)
    (work/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    if result.returncode:
      raise RuntimeError(f'provider comparison {name} failed: {result.returncode}')
    return json.loads((work/(name+'.json')).read_bytes())

  try:
    payload = sum(row['bytes'] for plan in pins.values() for row in plan['members'].values())
    growth = next(iter(pins.values()))['source_archive']['bytes'] + 10*payload + 128*1024**2
    assert shutil.disk_usage(work).free >= growth
    origin_plan = work/'origin-plan.json'
    origin_plan.write_text(json.dumps(dict(schema=1, reserve_bytes=0,
        assets={'original':next(iter(pins.values()))['source_archive']})))
    execute([args.assets, '--plan', origin_plan, '--cache', work/'raw'], 'prefetch')
    base = [args.preparer, '--export-released-provider-batch', '--source-root', source,
        '--store', work/'batch-cache']
    for plan in plans:
      base += ['--plan', plan]
    cold = execute([*base, '--cache', work/'raw', '--output', work/'cold-output'], 'cold')
    assert cold['source_archive_passes'] == 1 and cold['transferred_bytes'] == 0
    (work/'raw').rename(work/'raw-retained')
    warm = execute([*base, '--cache', work/'absent-raw', '--output', work/'warm-output'], 'warm')
    assert warm['source_archive_passes'] == warm['source_archive_bytes'] == warm['transferred_bytes'] == 0
    assert not (work/'absent-raw').exists()
    for profile, value in warm['exports'].items():
      assert value['origin']['copied_bytes'] == 0
      assert value['component_sha256'] == cold['exports'][profile]['component_sha256']
    summary['cold_all_profiles_seconds'] = summary['cases']['cold']['seconds']
    summary['warm_all_profiles_seconds'] = summary['cases']['warm']['seconds']
    summary['cold_including_prefetch_seconds'] = summary['cold_all_profiles_seconds'] + summary['cases']['prefetch']['seconds']
    print(json.dumps(dict(phase='preparation', cold_seconds=summary['cold_all_profiles_seconds'],
        warm_seconds=summary['warm_all_profiles_seconds'],
        cold_including_prefetch_seconds=summary['cold_including_prefetch_seconds'],
        equivalence_complete=False)), flush=True)
    # Do not spend another two hours comparing legacy exports after timing has
    # already failed. Every successful run still requires all fifteen below.
    assert summary['cold_all_profiles_seconds'] <= 12*60
    assert summary['cold_including_prefetch_seconds'] <= 12*60
    assert summary['warm_all_profiles_seconds'] <= 3*60
    companions = {'numerics', 'geos', 'brotli', 'acados', 'sdl', 'turbojpeg', 'ffmpeg-tools', 'freesans'}
    baseline_seconds = 0
    for path, pin in pins.items():
      name = path.name.removeprefix('native-').removesuffix('-provider.json')
      command = [args.preparer, '--export-released-provider', '--plan', path,
          '--cache', work/'raw-retained', '--store', work/'legacy-origins', '--output', work/('legacy-'+name)]
      if pin.get('source_contract_sha256'):
        command += ['--source-root', source]
      legacy = execute(command, 'legacy-'+name)
      current = cold['exports'][pin['profile']]
      assert legacy['origin']['asset']['transferred_bytes'] == 0
      assert legacy['tree_sha256'] == current['tree_sha256']
      assert digest(legacy['archive']) == digest(current['archive'])
      assert digest(legacy['descriptor']) == digest(current['descriptor'])
      summary['comparisons'][pin['profile']] = dict(tree=True, archive=True, descriptor=True)
      if name in companions:
        baseline_seconds += summary['cases']['legacy-'+name]['seconds']
      (work/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    summary['legacy_companions_seconds'] = baseline_seconds
    assert not subprocess.check_output(['git','-C',str(source),'status','--porcelain','--untracked-files=all'],text=True).strip()
    summary['success'] = True
  finally:
    (work/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
  print(json.dumps(summary))


if __name__ == '__main__':
  main()

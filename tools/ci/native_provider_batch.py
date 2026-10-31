#!/usr/bin/env python3
"""Select a job-owned verified batch export; retain legacy single consumers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


def export_component(preparer, plan_path, source, work, output, legacy_store, legacy_output):
  plan_path, source, work, output = map(Path, (plan_path, source, work, output))
  plan = json.loads(plan_path.read_bytes())
  batch_path = os.environ.get('NATIVE_PROVIDER_BATCH')
  if not batch_path:
    command = [str(preparer), '--export-released-provider', '--plan', str(plan_path),
        '--cache', str(work/'native-provider-origin'), '--store', str(legacy_store), '--output', str(legacy_output)]
    if plan.get('source_contract_sha256'):
      command += ['--source-root', str(source)]
    with output.open('x') as stream:
      subprocess.run(command, stdout=stream, check=True)
    return json.loads(output.read_bytes())
  path = Path(batch_path)
  pin = os.environ.get('NATIVE_PROVIDER_BATCH_SHA256', '')
  if (not re.fullmatch('[a-f0-9]{64}', pin) or path.is_symlink() or not path.is_file()
      or not path.resolve().is_relative_to(work.resolve()) or path.stat().st_size > 1024**2):
    raise ValueError('provider batch receipt is outside this job or lacks an independent digest')
  raw = path.read_bytes()
  if hashlib.sha256(raw).hexdigest() != pin:
    raise ValueError('provider batch receipt changed during the job')
  batch = json.loads(raw)
  if batch['schema'] != 1 or batch['source_archive_sha256'] != plan['source_archive']['sha256']:
    raise ValueError('provider batch uses a different original source archive')
  value = batch['exports'][plan['profile']]
  if value['provider_source_commit'] != plan['source_commit'] or value['published_release'] or value['provider_executed']:
    raise ValueError('provider batch source or publication boundary differs')
  for field, expected in [('descriptor', value['descriptor_sha256']), ('archive', value['component_sha256'])]:
    asset = Path(value[field])
    if asset.is_symlink() or not asset.is_file() or not asset.resolve().is_relative_to(work.resolve()):
      raise ValueError('provider batch component escaped this job')
    with asset.open('rb') as stream:
      if hashlib.file_digest(stream, 'sha256').hexdigest() != expected:
        raise ValueError('provider batch component changed before consumption')
  if Path(value['archive']).stat().st_size != value['component_bytes']:
    raise ValueError('provider batch component size changed')
  if plan.get('source_contract_sha256'):
    binding = value['source_binding']
    head = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if (binding['source']['commit'] != head or binding['contract_sha256'] != plan['source_contract_sha256']
        or not binding['inputs_match'] or binding['regenerated']):
      raise ValueError('provider batch source contract differs from the current checkout')
  with output.open('x') as stream:
    json.dump(value, stream, indent=2)
    stream.write('\n')
  return value


def main():
  parser = argparse.ArgumentParser()
  for name in ('preparer', 'plan', 'root', 'work', 'output', 'legacy-store', 'legacy-output'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  export_component(args.preparer, args.plan, args.root, args.work, args.output, args.legacy_store, args.legacy_output)


if __name__ == '__main__':
  main()

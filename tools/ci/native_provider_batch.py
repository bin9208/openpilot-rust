#!/usr/bin/env python3
"""Select a job-owned verified batch export; retain legacy single consumers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
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
  # The original AGNOS PRoot namespace binds these per-profile export paths.
  # Materialize only verified small files there; cache locations are not guest
  # paths. Keep Export's descriptor/archive addresses compatible with its old
  # single-provider caller and preserve any pre-existing output.
  legacy_output = Path(legacy_output)
  if not legacy_output.resolve().is_relative_to(work.resolve()):
    raise ValueError('provider export view is outside this job')
  legacy_output.mkdir()
  opened = False
  try:
    value = dict(value)
    for field, pin_field in [('descriptor', 'descriptor_sha256'), ('archive', 'component_sha256')]:
      original = Path(value[field])
      destination = legacy_output/('provider.json' if field == 'descriptor' else original.name)
      with original.open('rb') as reader, destination.open('xb') as writer:
        shutil.copyfileobj(reader, writer)
        writer.flush()
        os.fsync(writer.fileno())
      with destination.open('rb') as reader:
        if hashlib.file_digest(reader, 'sha256').hexdigest() != value[pin_field]:
          raise ValueError('provider changed while materializing its guest export')
      destination.chmod(0o444)
      value[field] = str(destination)
    with output.open('x') as stream:
      opened = True
      json.dump(value, stream, indent=2)
      stream.write('\n')
      stream.flush()
      os.fsync(stream.fileno())
  except BaseException:
    shutil.rmtree(legacy_output)
    if opened: output.unlink()
    raise
  return value


def main():
  parser = argparse.ArgumentParser()
  for name in ('preparer', 'plan', 'root', 'work', 'output', 'legacy-store', 'legacy-output'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  export_component(args.preparer, args.plan, args.root, args.work, args.output, args.legacy_store, args.legacy_output)


if __name__ == '__main__':
  main()

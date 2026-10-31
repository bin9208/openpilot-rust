#!/usr/bin/env python3
"""Prepare all fixed provider components through one original gzip pass."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess


def main():
  parser = argparse.ArgumentParser()
  for name in ('preparer', 'root', 'work'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_environment']()
  source, work = args.root.resolve(), args.work.resolve()
  assert source == Path(os.environ['GITHUB_WORKSPACE']).resolve()
  assert work == Path(os.environ['CI_JOB_TEMP']).resolve()
  head = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
  assert head == os.environ['GITHUB_SHA']
  plans = sorted((source/'rust').glob('native-*-provider.json'))
  assert len(plans) == 15
  command = [str(args.preparer), '--export-released-provider-batch', '--source-root', str(source),
      '--cache', str(work/'native-provider-origin'), '--store', str(work/'native-provider-batch-cache'),
      '--output', str(work/'native-provider-batch-result')]
  for path in plans:
    command += ['--plan', str(path)]
  output = work/'native-provider-batch.json'
  with output.open('x') as stream:
    subprocess.run(command, stdout=stream, check=True)
  value = json.loads(output.read_bytes())
  expected = {json.loads(path.read_bytes())['profile'] for path in plans}
  assert set(value['exports']) == expected
  assert value['source_archive_passes'] in (0, 1)
  if value['source_archive_passes'] == 0:
    assert value['transferred_bytes'] == value['source_archive_bytes'] == 0
  for exported in value['exports'].values():
    assert not exported['published_release'] and not exported['provider_executed']
    assert exported['origin']['asset']['transferred_bytes'] == exported['origin']['asset']['stored_bytes'] == 0
  pin = hashlib.sha256(output.read_bytes()).hexdigest()
  with open(os.environ['GITHUB_ENV'], 'a') as stream:
    stream.write(f'NATIVE_PROVIDER_BATCH={output}\nNATIVE_PROVIDER_BATCH_SHA256={pin}\n')
    stream.write(f'NATIVE_PROVIDER_BATCH_ORIGIN_USED={str(value["source_archive_passes"] == 1).lower()}\n')
  print(json.dumps({key:value[key] for key in ('source_archive_passes', 'source_archive_bytes', 'transferred_bytes', 'elapsed_ms')}))


if __name__ == '__main__':
  main()

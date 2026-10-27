#!/usr/bin/env python3
"""Actions-only production of a foreign-only candidate and real prefix reuse."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--provider-preparer', type=Path, required=True)
  parser.add_argument('--prefix-preparer', type=Path, required=True)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--work', type=Path, required=True)
  args = parser.parse_args()
  work = args.work.resolve()
  plan = args.root/'rust/native-opencv-provider.json'
  pins = json.loads(plan.read_text())
  payload = sum(row['bytes'] for row in pins['members'].values())
  growth = payload*4+16*1024**2
  free = shutil.disk_usage(work).free
  print(json.dumps(dict(available_bytes=free, expected_additional_bytes=growth)), flush=True)
  if free < growth:
    raise RuntimeError('insufficient capacity for this component/prefix operation; other data retained')

  def execute(command, name):
    path = work/f'native-opencv-{name}.json'
    with path.open('x') as output:
      subprocess.run(list(map(str, command)), stdout=output, check=True)
    return json.loads(path.read_text())

  export = execute([args.provider_preparer, '--export-released-provider', '--plan', plan,
      '--cache', work/'native-provider-origin', '--store', work/'native-provider-origins',
      '--output', work/'native-opencv-provider-export'], 'provider-export')
  assert export['origin']['asset']['transferred_bytes'] == export['origin']['asset']['stored_bytes'] == 0
  assert not export['published_release'] and not export['provider_executed']
  consume = [args.provider_preparer, '--prepare-released-provider', '--plan', plan,
      '--descriptor', export['descriptor'], '--descriptor-sha256', export['descriptor_sha256'],
      '--cache', work/'native-opencv-components', '--store', work/'native-opencv-provider-prefixes',
      '--offline-archive', export['archive']]
  providers = []
  for phase in ('initial', 'reused'):
    row = execute(consume, f'provider-{phase}')
    assert row['tree_sha256'] == export['tree_sha256']
    assert row['asset']['transferred_bytes'] == row['asset']['stored_bytes'] == 0
    assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
    assert (row['copied_bytes'] > 0) == (phase == 'initial')
    assert (row['offline_seed_bytes'] > 0) == (phase == 'initial')
    providers.append(row)
  command = [args.prefix_preparer, '--prepare-released-prefix',
      '--header-plan', args.root/'rust/native-opencv-headers.json', '--provider-plan', plan,
      '--descriptor', export['descriptor'], '--descriptor-sha256', export['descriptor_sha256'],
      '--header-cache', work/'native-opencv-source', '--provider-cache', work/'native-opencv-components',
      '--store', work/'native-opencv-prefixes']
  rows = []
  for phase in ('initial', 'reused'):
    row = execute(command, f'prefix-{phase}')
    assert not row['compiler_executed'] and not row['runtime_activated']
    assert row['provider']['tree_sha256'] == export['tree_sha256']
    for part in ('headers', 'provider'):
      assert row[part]['asset']['transferred_bytes'] == row[part]['asset']['stored_bytes'] == 0
    if phase == 'initial':
      assert row['linked_bytes'] > 0
    else:
      assert row['copied_bytes'] == row['linked_bytes'] == 0
      assert row['headers']['copied_bytes'] == row['provider']['copied_bytes'] == 0
      assert not row['headers']['source_archive_decompressed']
    rows.append(row)
  assert rows[0]['root'] == rows[1]['root'] and rows[0]['tree_sha256'] == rows[1]['tree_sha256']
  prefix = Path(rows[0]['root'])
  manifest = prefix/'native-libraries.sha256'
  assert hashlib.sha256(manifest.read_bytes()).hexdigest() == 'bdc3c7f0ea23ea1e15272906478d92433997c8f36a4fe2bd39ab167211be2d06'
  assert len(manifest.read_text().splitlines()) == 185
  subprocess.run(['sha256sum', '--check', manifest], cwd=prefix, check=True)
  result = dict(schema=1, provider_tree_sha256=export['tree_sha256'],
      prefix_tree_sha256=rows[0]['tree_sha256'], descriptor_sha256=export['descriptor_sha256'],
      selected_members=30, original_library_files=5, aliases=10, original_headers=170,
      native_manifest_rows=185, origin_repeated_http_body_bytes=0,
      consumer_origin_archive_downloaded=False, repeated_http_body_bytes=0,
      repeated_copied_bytes=0, repeated_linked_bytes=0, repeated_source_decompression=False,
      published_release=False, public_component_download_tested=False,
      compiler_executed=False, provider_executed=False, project_binaries_reused=False,
      physical_device_execution=False)
  with (work/'native-opencv-prefix-summary.json').open('x') as output:
    json.dump(result, output, indent=2)
    output.write('\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

#!/usr/bin/env python3
"""Actions-only checks of the Rust preparer and a job-owned cache forgery."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--preparer', type=Path, required=True)
  parser.add_argument('--plan', type=Path, required=True)
  parser.add_argument('--work', type=Path, required=True)
  args = parser.parse_args()
  work = args.work.resolve()
  command = list(map(str, [args.preparer, '--prepare-released-headers', '--plan', args.plan,
      '--cache', work/'native-opencv-source', '--store', work/'native-opencv-headers']))

  def prepare(phase):
    path = work/f'native-opencv-headers-{phase}.json'
    with path.open('x') as output:
      subprocess.run(command, stdout=output, check=True)
    row = json.loads(path.read_text())
    assert row['public_headers'] == 170
    assert not row['compiler_executed'] and not row['runtime_activated']
    assert not row['provider_libraries_prepared'] and not row['actual_target_execution']
    return row

  first = prepare('initial')
  repeat = prepare('reused')
  assert first['asset']['sha256'] == '6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61'
  assert first['asset']['bytes'] == 95450266
  assert first['asset']['transferred_bytes'] in (0, 95450266)
  assert first['source_archive_decompressed'] and not repeat['source_archive_decompressed']
  assert first['copied_bytes'] > 0 and repeat['copied_bytes'] == 0
  assert repeat['asset']['transferred_bytes'] == repeat['asset']['stored_bytes'] == 0
  assert first['root'] == repeat['root'] and first['tree_sha256'] == repeat['tree_sha256']
  root = Path(first['root'])
  tree_path = Path(first['tree_receipt'])
  receipt = root.parent/'receipt.json'
  tree = json.loads(tree_path.read_text())
  headers = {path: entry for path, entry in tree['entries'].items()
      if path.startswith('install/include/opencv4/') and entry['kind'] == 'file'}
  assert len(headers) == 170
  assert not any(path.startswith(('bin/', 'install/lib/')) for path in tree['entries'])
  for path, expected in (('opencv2/cvconfig.h', 'd86babae9fcae249a465ba2358837dba91c78a1d52e18639df9790c1b7377d2a'),
      ('opencv2/opencv_modules.hpp', '3cc030a42cb62ed2d01e8d540bc87433499b1ac9cba204538262dfd816134077')):
    assert hashlib.sha256((root/'install/include/opencv4'/path).read_bytes()).hexdigest() == expected
  original = root/'provenance/original-opencv-native-libraries.sha256'
  assert hashlib.sha256(original.read_bytes()).hexdigest() == 'd34c38a3ae14a51624759d526a825e596297ff203ad3ad6861cd08a32798934d'

  # Modify only this job's freshly created header prefix and its two receipts.
  header = root/'install/include/opencv4/opencv2/cvconfig.h'
  saved = {p: (p.read_bytes(), p.stat().st_mode & 0o7777) for p in (header, tree_path, receipt)}

  def replace(path, data):
    path.chmod(0o644)
    try:
      path.write_bytes(data)
    finally:
      path.chmod(saved[path][1])

  try:
    data = bytearray(saved[header][0])
    data[0] ^= 1
    replace(header, data)
    tree['entries']['install/include/opencv4/opencv2/cvconfig.h']['sha256'] = hashlib.sha256(data).hexdigest()
    forged = (json.dumps(tree, indent=2)+'\n').encode()
    replace(tree_path, forged)
    replace(receipt, (json.dumps([1, first['identity'], hashlib.sha256(forged).hexdigest()], indent=2)+'\n').encode())
    with (work/'native-opencv-headers-tamper.stdout').open('xb') as output, (work/'native-opencv-headers-tamper.stderr').open('xb') as error:
      rejected = subprocess.run(command, stdout=output, stderr=error, check=False)
    assert rejected.returncode != 0
    assert 'metadata differs from its authenticated archive' in (work/'native-opencv-headers-tamper.stderr').read_text()
  finally:
    for path, (data, _) in saved.items():
      replace(path, data)
  after = prepare('after-recovery')
  assert after['tree_sha256'] == first['tree_sha256']
  assert after['copied_bytes'] == after['asset']['transferred_bytes'] == after['asset']['stored_bytes'] == 0
  assert not after['source_archive_decompressed']
  result = dict(schema=1, tree_sha256=first['tree_sha256'], public_headers=170,
      source_headers=168, original_generated_headers=2, original_hardening_preserved=True,
      initial_http_body_bytes=first['asset']['transferred_bytes'], repeated_http_body_bytes=0,
      repeated_copied_bytes=0, repeated_source_decompression=False,
      same_size_generated_header_and_both_receipts_forgery_rejected=True, original_prefix_restored=True,
      compiler_executed=False, provider_libraries_prepared=False, physical_device_execution=False)
  with (work/'native-opencv-headers-summary.json').open('x') as output:
    json.dump(result, output, indent=2)
    output.write('\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

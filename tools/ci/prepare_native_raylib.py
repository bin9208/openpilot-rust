#!/usr/bin/env python3
"""Actions-only orchestration of the original Rust raylib input preparer."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--preparer', type=Path, required=True)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--work', type=Path, required=True)
  args = parser.parse_args()
  work = args.work.resolve()
  command = list(map(str, [args.preparer, '--prepare-native-inputs', '--root', args.root,
      '--cache', work/'native-raylib-assets', '--store', work/'native-raylib-sdk-prefixes']))

  def prepare(phase):
    path = work/f'native-raylib-sdk-{phase}.json'
    with path.open('x') as output:
      subprocess.run(command, stdout=output, check=True)
    row = json.loads(path.read_text())
    assert not row['compiler_executed'] and not row['runtime_activated']
    return row

  first = prepare('initial')
  repeat = prepare('reused')
  assert first['asset']['sha256'] == 'd9b9bebb39167442bacf627958d57ae95e452550ced1b68337cbf2a73375856a'
  assert first['asset']['bytes'] == 21707647
  assert first['asset']['transferred_bytes'] in (0, 21707647)
  assert first['tree_sha256'] == repeat['tree_sha256'] and first['root'] == repeat['root']
  assert first['copied_bytes'] > 0
  assert repeat['copied_bytes'] == repeat['asset']['transferred_bytes'] == repeat['asset']['stored_bytes'] == 0
  root = Path(first['root'])
  tree_path = Path(first['tree_receipt'])
  receipt_path = root.parent/'receipt.json'
  tree = json.loads(tree_path.read_text())
  native = {p for p in tree['entries'] if p.startswith(('include/', 'lib/'))}
  assert native == {'include/raylib.h', 'include/raymath.h', 'include/rlgl.h', 'lib/libraylib_comma.a'}
  assert not any(p.endswith(('.py', '.so', '.so.1', '.so.2')) for p in tree['entries'])
  assert tree['entries']['include/raylib.h']['sha256'] == '047e7255f93f8c34039cab906ad76136706b5c7b4c5b5b065d84141963ee9b6b'
  for name in ('METADATA', 'WHEEL', 'RECORD'):
    assert (root/'provenance/wheel-dist-info'/name).is_file()

  # Mutate only this job's new prefix; retain the source raw cache and all
  # unrelated providers. Forge both local receipts to exercise raw authority.
  header = root/'include/raylib.h'
  saved = {p: (p.read_bytes(), p.stat().st_mode & 0o7777) for p in (header, tree_path, receipt_path)}

  def replace(path, data):
    mode = saved[path][1]
    path.chmod(0o644)
    try:
      path.write_bytes(data)
    finally:
      path.chmod(mode)

  try:
    data = bytearray(saved[header][0])
    data[0] ^= 1
    replace(header, data)
    tree['entries']['include/raylib.h']['sha256'] = hashlib.sha256(data).hexdigest()
    forged = (json.dumps(tree, indent=2)+'\n').encode()
    replace(tree_path, forged)
    replace(receipt_path, (json.dumps([1, first['identity'], hashlib.sha256(forged).hexdigest()], indent=2)+'\n').encode())
    with (work/'native-raylib-sdk-tamper.stdout').open('xb') as output, (work/'native-raylib-sdk-tamper.stderr').open('xb') as error:
      rejected = subprocess.run(command, stdout=output, stderr=error, check=False)
    assert rejected.returncode != 0
    assert 'metadata differs from its authenticated archive' in (work/'native-raylib-sdk-tamper.stderr').read_text()
  finally:
    for path, (data, _) in saved.items():
      replace(path, data)
  recovered = prepare('after-recovery')
  assert recovered['tree_sha256'] == first['tree_sha256']
  assert recovered['copied_bytes'] == recovered['asset']['transferred_bytes'] == recovered['asset']['stored_bytes'] == 0
  result = dict(schema=1, tree_sha256=first['tree_sha256'],
      initial_http_body_bytes=first['asset']['transferred_bytes'], repeated_http_body_bytes=0,
      repeated_copied_bytes=0, native_payload_files=4, python_and_unrelated_mesa_excluded=True,
      same_size_header_and_both_receipts_forgery_rejected=True, original_prefix_restored=True,
      compiler_executed=False, runtime_activated=False, physical_device_execution=False)
  with (work/'native-raylib-sdk-summary.json').open('x') as output:
    json.dump(result, output, indent=2)
    output.write('\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()

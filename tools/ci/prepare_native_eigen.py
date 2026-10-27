#!/usr/bin/env python3
"""Actions orchestration only: exercise the source-built Rust wheel preparer."""
import argparse
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

  def prepare(phase, selected, cache, store):
    output = work / f'{phase}.json'
    command = [args.preparer, '--prepare-wheels', '--root', args.root, '--packages', selected,
               '--cache', cache, '--store', store]
    with output.open('x') as stream:
      subprocess.run(list(map(str, command)), stdout=stream, check=True)
    result = json.loads(output.read_text())
    assert set(result) == set(selected.split(','))
    assert all(not row['compiler_executed'] and not row['runtime_activated'] for row in result.values())
    return result

  first = prepare('native-eigen-initial', 'eigen', work/'native-eigen-assets', work/'native-eigen-prefixes')['eigen']
  repeated = prepare('native-eigen-reused', 'eigen', work/'native-eigen-assets', work/'native-eigen-prefixes')['eigen']
  assert first['asset']['sha256'] == 'b02ee704aa964d9d7bafb53cc0e096680e4a3b4f6fb1cd98b0a85c7298f3dc0a'
  assert first['asset']['bytes'] == 2250657
  assert first['asset']['transferred_bytes'] in (0, 2250657)
  assert first['root'] == repeated['root'] and first['tree_sha256'] == repeated['tree_sha256']
  assert first['copied_bytes'] > 0
  assert repeated['copied_bytes'] == repeated['asset']['transferred_bytes'] == repeated['asset']['stored_bytes'] == 0
  prefix = Path(first['root'])
  assert (prefix/'eigen3/Eigen/Core').is_file()
  entries = json.loads(Path(first['tree_receipt']).read_text())['entries']
  assert not any(name.endswith('.py') for name in entries), 'Python shim entered native header prefix'
  assert all((prefix/'provenance/wheel-dist-info'/name).is_file() for name in ('METADATA', 'WHEEL', 'RECORD'))

  # The original pair was prepared by the preceding unchanged workflow step.
  # Selecting its libyuv alone must reuse that exact prefix and raw cache.
  original = json.loads((work/'native-codec-first.json').read_text())['libyuv']
  yuv = prepare('native-libyuv-only', 'libyuv', work/'native-codec-assets', work/'native-codec-prefixes')['libyuv']
  assert yuv['identity'] == original['identity'] and yuv['tree_sha256'] == original['tree_sha256']
  assert yuv['root'] == original['root']
  assert yuv['copied_bytes'] == yuv['asset']['transferred_bytes'] == yuv['asset']['stored_bytes'] == 0
  summary = dict(schema=1, eigen_tree_sha256=first['tree_sha256'],
      eigen_source_archive_sha256=first['asset']['sha256'], eigen_archive_bytes=2250657,
      initial_eigen_http_body_bytes=first['asset']['transferred_bytes'],
      repeated_eigen_http_body_bytes=0, repeated_eigen_copied_bytes=0,
      selected_only_eigen=True, libyuv_only_reuses_original_default_prefix=True,
      selected_libyuv_http_body_bytes=0, selected_libyuv_copied_bytes=0,
      compiler_executed=False, runtime_activated=False, device_execution=False)
  with (work/'native-eigen-summary.json').open('x') as output:
    json.dump(summary, output, indent=2)
    output.write('\n')
  print(json.dumps(summary))


if __name__ == '__main__':
  main()

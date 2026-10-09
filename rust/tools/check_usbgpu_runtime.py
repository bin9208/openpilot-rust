"""Compare complete original/native USB GPU initialization against owned hardware."""

from __future__ import annotations
import argparse
import json
from pathlib import Path
import subprocess
import sys
from check_usbgpu_asic import owned_pte_reads, compare_with_source_rereads

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(exist_ok=True, parents=True)
  rows = []
  for custom in [True, False]:
    for aql in [False, True]:
      for no_copy in [False, True]:
        case = f'{"custom" if custom else "stock"}-aql{int(aql)}-nocopy{int(no_copy)}'
        loaded, invocations = {}, []
        for kind in ['source', 'native']:
          output = args.evidence / f'{kind}-{case}.json'
          command = [
            sys.executable,
            str(ROOT / 'rust/tools/usbgpu_runtime_fixture.py'),
            '--kind',
            kind,
            '--firmware',
            str(args.firmware),
            '--output',
            str(output),
          ]
          if kind == 'native':
            command.extend(['--binary', args.binary])
          if not custom:
            command.append('--stock')
          if aql:
            command.append('--aql')
          if no_copy:
            command.append('--no-copy')
          process = subprocess.run(command, text=True, capture_output=True, timeout=60)
          (args.evidence / f'{kind}-{case}.log').write_text(process.stdout + process.stderr)
          assert process.returncode == 0, process.stderr
          loaded[kind] = json.loads(output.read_text())
          invocations.append({'command': command, 'exit_code': process.returncode, 'artifact': str(output)})
        source, native = loaded['source'], loaded['native']
        source_trace, reads = owned_pte_reads(source['trace'])
        native_trace, _ = owned_pte_reads(native['trace'])
        matched, skipped, mismatch = compare_with_source_rereads(source['trace'], native['trace'], reads)
        passed = matched and source_trace == native_trace and source['result'] == native['result']['result'] and native['result']['exit_code'] == 0
        rows.append(
          {
            'case': case,
            'passed': passed,
            'source_result': source['result'],
            'native_result': native['result'],
            'source_events': len(source['trace']),
            'native_events': len(native['trace']),
            'source_rereads_skipped': skipped,
            'mismatch': mismatch,
            'invocations': invocations,
          }
        )
        print(case, 'PASS' if passed else 'FAIL', len(native['trace']), 'native events', flush=True)
  (args.evidence / 'comparison.json').write_text(json.dumps({'results': rows}, indent=2) + '\n')
  assert all(row['passed'] for row in rows), [row['case'] for row in rows if not row['passed']]


if __name__ == '__main__':
  main()

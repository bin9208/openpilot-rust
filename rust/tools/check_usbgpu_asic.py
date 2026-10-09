"""Run original/native AMD boot against an owned hardware model and compare I/O."""

from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def owned_pte_reads(trace):
  values, zero_ranges, normalized, removed = {}, [], [], []
  for index, event in enumerate(trace):
    if event['op'] == 'vram_write':
      start, end = event['address'], event['address'] + event['size']
      values = {address: value for address, value in values.items() if address + 8 <= start or address >= end}
      zero_ranges = [(lo, hi) for lo, hi in zero_ranges if hi <= start or lo >= end]
      if event['sha256'] == hashlib.sha256(bytes(event['size'])).hexdigest():
        zero_ranges.append((start, end))
    elif event['op'] == 'value_write' and event['size'] == 8:
      values[event['address']] = event['value']
    elif event['op'] == 'value_read' and event['size'] == 8:
      address = event['address']
      expected = values.get(address, 0 if any(lo <= address and address + 8 <= hi for lo, hi in zero_ranges) else None)
      assert expected is not None and event['result'] == expected, ('read is not an owned unchanged PTE', event, expected)
      removed.append({'event': index, 'address': address, 'value': expected})
      continue
    normalized.append(event)
  return normalized, removed


def compare_with_source_rereads(source, native, audited):
  allowed = {event['event'] for event in audited}
  index, skipped = 0, []
  for native_index, event in enumerate(native):
    while index < len(source) and source[index] != event and index in allowed:
      skipped.append(index)
      index += 1
    if index >= len(source) or source[index] != event:
      return False, skipped, {'source_index': index, 'native_index': native_index}
    index += 1
  while index < len(source) and index in allowed:
    skipped.append(index)
    index += 1
  return index == len(source), skipped, None


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  cases = [(initial, power, 'none', False) for initial in ['cold', 'warm', 'dirty'] for power in [None, 42.5]]
  cases += [('cold', None, 'psp_error', False), ('cold', None, 'none', True)]
  results = []
  for initial, power, fault, queues in cases:
    case = f'{initial}-{power}-{fault}' + ('-queues' if queues else '')
    loaded, invocations = {}, []
    for kind in ['source', 'native']:
      output = args.evidence / f'{kind}-{case}.json'
      command = [
        sys.executable,
        str(ROOT / 'rust/tools/usbgpu_asic_fixture.py'),
        '--kind',
        kind,
        '--initial',
        initial,
        '--fault',
        fault,
        '--firmware',
        str(args.firmware),
        '--output',
        str(output),
      ]
      if kind == 'native':
        command.extend(['--binary', args.binary])
      if power is not None:
        command.extend(['--power', str(power)])
      if queues:
        command.append('--queues')
      process = subprocess.run(command, text=True, capture_output=True, timeout=60)
      (args.evidence / f'{kind}-{case}.log').write_text(process.stdout + process.stderr)
      assert process.returncode == 0, process.stderr
      loaded[kind] = json.loads(output.read_text())
      invocations.append({'command': command, 'exit_code': process.returncode, 'artifact': str(output)})
    source, native = loaded['source'], loaded['native']
    original, source_removed = owned_pte_reads(source['trace'])
    actual, native_removed = owned_pte_reads(native['trace'])
    source_result, native_result = source['result'], native['result']['result']
    expected_code = 1 if 'error' in source_result else 0
    matched, source_skipped, mismatch = compare_with_source_rereads(source['trace'], native['trace'], source_removed)
    passed = matched and original == actual and source_result == native_result and native['result']['exit_code'] == expected_code
    differences = [(index, left, right) for index, (left, right) in enumerate(zip(original, actual, strict=False)) if left != right]
    results.append(
      {
        'case': case,
        'passed': passed,
        'invocations': invocations,
        'source_result': source_result,
        'native_result': native_result,
        'source_events': len(source['trace']),
        'native_events': len(native['trace']),
        'compared_events': len(native['trace']),
        'source_rereads_skipped': source_skipped,
        'raw_alignment_mismatch': mismatch,
        'source_owned_pte_reads': source_removed,
        'native_owned_pte_reads': native_removed,
        'first_difference': differences[:1],
      }
    )
    print(case, 'PASS' if passed else 'FAIL', len(native['trace']), 'compared events', flush=True)
  (args.evidence / 'comparison.json').write_text(
    json.dumps(
      {
        'normalization': (
          'Every native event must match the original sequence. Only extra source PTE rereads, '
          + 'independently verified against prior owned writes/zeroes, may be skipped. Native reads and all other I/O remain compared.'
        ),
        'results': results,
      },
      indent=2,
    )
    + '\n'
  )
  assert all(result['passed'] for result in results), [result['case'] for result in results if not result['passed']]


if __name__ == '__main__':
  main()

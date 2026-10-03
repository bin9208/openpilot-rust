from __future__ import annotations

import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
from urllib.parse import quote

from check_camerad_kernel import digest, disable_core
from check_camerad_isp_lifecycle import normalize
from camerad_camera_lifecycle_cleanup import audit, normalize_disabled


def images(records: list[dict]) -> tuple[list[dict], dict[int, int]]:
  identities = {}
  result = deepcopy(records)
  for row in result:
    if row['op'] in ('image-import', 'vision-open'):
      fd = row['fd']
      identities.setdefault(fd, len(identities) + 1)
      row['fd'] = identities[fd]
    if row['op'] in ('close', 'vision-mmap', 'vision-munmap') and row['fd'] in identities:
      row['fd'] = identities[row['fd']]
    if row.get('name') == 'camera:275':
      for key in ('before', 'after'):
        payload = bytearray.fromhex(row[key])
        fd = int.from_bytes(payload[72:76], 'little')
        payload[72:76] = identities[fd].to_bytes(4, 'little')
        row[key] = payload.hex()
  return result, identities


def result_value(stdout: str, native: bool):
  value = json.loads(stdout)
  if native:
    for event in value['events']:
      if event['frame'] is not None:
        frame = event['frame']
        event['frame'] = [frame['slot'], frame['frame_id'], frame['request_id'], frame['timestamp_sof'],
                          frame['timestamp_eof'], struct.unpack('<I', struct.pack('<f', frame['processing_time']))[0]]
  return value


def run(args, lane: str, binary: Path, name: str, arguments: list[str], overrides: dict) -> tuple[dict, list[dict]]:
  folder = args.output / quote(name, safe='-_.') / lane
  folder.mkdir(parents=True)
  trace = folder / 'trace.jsonl'
  with tempfile.TemporaryDirectory(prefix='msgq_camera_lifecycle_', dir='/dev/shm') as namespace:
    env = {**os.environ, **overrides, 'CK_TRACE': str(trace), 'OPENPILOT_PREFIX': Path(namespace).name.removeprefix('msgq_')}
    for key in ('CEREAL_FAKE', 'SPECTRA_ERROR_PROB', 'SPECTRA_ERROR_DT'):
      env.pop(key, None)
    command = [str(binary), *arguments]
    if args.qemu:
      assert args.sysroot
      command = [str(args.qemu), '-L', str(args.sysroot), '-E', f'LD_PRELOAD={args.fixture}', *command]
    else:
      asan = '/usr/lib/llvm-18/lib/clang/18/lib/linux'
      env.update(LD_PRELOAD=f'{asan}/libclang_rt.asan-x86_64.so:{args.fixture}', LD_LIBRARY_PATH=asan,
                 ASAN_OPTIONS='detect_leaks=0:abort_on_error=1', UBSAN_OPTIONS='halt_on_error=1:print_stacktrace=1')
    process = subprocess.run(command, env=env, capture_output=True, text=True, timeout=90, preexec_fn=disable_core)
  (folder / 'stdout').write_text(process.stdout)
  (folder / 'stderr').write_text(process.stderr)
  records = [json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else []
  metadata = dict(command=command, environment={key: value for key, value in env.items()
                  if key.startswith(('CK_', 'ASAN_', 'UBSAN_', 'LD_', 'OPENPILOT_'))}, exit=process.returncode,
                  stdout=process.stdout, stderr_sha256=digest(folder / 'stderr'), trace_sha256=digest(trace) if trace.exists() else None,
                  sanitizer_failure=any(text in process.stderr for text in ('ERROR: AddressSanitizer', 'runtime error:', 'Sanitizer CHECK failed')))
  (folder / 'run.json').write_text(json.dumps(metadata, indent=2) + '\n')
  return metadata, records


def main() -> None:
  parser = argparse.ArgumentParser(description='Compare original complete camera lifecycle, image imports, packet bytes and event outcomes')
  for name in ('source', 'native', 'fixture', 'output'):
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--qemu', type=Path)
  parser.add_argument('--sysroot', type=Path)
  parser.add_argument('--smoke', action='store_true')
  parser.add_argument('--case', action='append')
  parser.add_argument('--cleanup-only', action='store_true')
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  free = shutil.disk_usage(args.output).free
  assert free >= 25*1024**3 + 1024**3
  sensors = [('ar', 0x354), ('ox', 0x5803), ('os', 0x5304)]
  cases = [(f'{sensor}-mode-{mode}-depth-{depth}', [str(mode), str(depth), '3'], {'CK_SENSOR': str(sensor_id)})
           for sensor, sensor_id in sensors for mode in range(3) for depth in (1, 18)]
  if args.smoke:
    cases = [cases[2], cases[-2]]
  else:
    for mode in (1, 2):
      for event in ('stale', 'frame-gap', 'request-gap', 'invalid'):
        count = 22 if event == 'invalid' else 4
        cases.append((f'events-{mode}-{event}', [str(mode), '18', str(count)], {'CK_SENSOR': str(0x5304), 'CK_EVENT_CASE': event}))
      for operation in ('sync:6', 'sync:1', 'camera:271'):
        cases.append((f'error-{mode}-{operation}', [str(mode), '18', '3'],
                      {'CK_SENSOR': str(0x5304), 'CK_FAIL_OP': operation, 'CK_FAIL_COUNT': '1', 'CK_FAIL_ERRNO': '5'}))
  if args.cleanup_only:
    common = {'CK_SENSOR': str(0x5304)}
    failure = {**common, 'CK_FAIL_COUNT': '1', 'CK_FAIL_ERRNO': '5'}
    cases = [
      ('cleanup-disabled', ['1', '1', '0'], {**common, 'CK_ENABLED': '0', 'CK_CLEANUP_KIND': 'disabled'}),
      ('cleanup-probe-none', ['1', '1', '0'], {'CK_SENSOR': '-1', 'CK_CLEANUP_KIND': 'probe-none'}),
      ('cleanup-sensor-init', ['1', '1', '0'], {**failure, 'CK_FAIL_OP': 'camera:261', 'CK_FAIL_TARGET': '502',
          'CK_FAIL_PACKET': '4', 'CK_CLEANUP_KIND': 'sensor-init'}),
      *[(f'cleanup-sensor-poke-{mode}', [str(mode), '18', '0'], {**failure, 'CK_FAIL_OP': 'camera:261', 'CK_FAIL_TARGET': '502',
          'CK_FAIL_PACKET': '127', 'CK_CLEANUP_KIND': 'sensor-poke'}) for mode in (1, 2)],
      ('cleanup-isp-acquire', ['1', '1', '0'], {**failure, 'CK_FAIL_OP': 'camera:258', 'CK_FAIL_TARGET': '503', 'CK_EXPECT_SOURCE_ABORT': '1'}),
      ('cleanup-phy-config', ['1', '1', '0'], {**failure, 'CK_FAIL_OP': 'camera:261', 'CK_FAIL_TARGET': '506', 'CK_EXPECT_SOURCE_ABORT': '1'}),
      ('cleanup-raw-map', ['2', '2', '0'], {**failure, 'CK_FAIL_OP': 'camera:275', 'CK_FAIL_TARGET': '501',
          'CK_FAIL_SKIP': '2', 'CK_EXPECT_SOURCE_ABORT': '1'}),
    ]
  if args.case:
    cases = [case for case in cases if case[0] in args.case]
    assert cases
  report = dict(binaries={str(path): digest(path) for path in (args.source, args.native, args.fixture)},
                free_bytes=free, estimated_growth_bytes=1024**3, cases=[], failures=[])
  for name, arguments, overrides in cases:
    source, native = [run(args, lane, binary, name, arguments, overrides) for lane, binary in (('source', args.source), ('native', args.native))]
    cleaned, ownership, source_ids, native_ids, error = native[1], {}, {}, {}, None
    resources, extra_cleanup = {}, []
    expected_abort = overrides.get('CK_EXPECT_SOURCE_ABORT') == '1'
    source_rows = source[1]
    try:
      resources = audit(native[1])
      cleaned, ownership = normalize(source[1], native[1])
      if kind := overrides.get('CK_CLEANUP_KIND'):
        cleaned, extra_cleanup = normalize_disabled(source[1], cleaned, kind)
      source_rows, source_ids = images(source[1])
      cleaned, native_ids = images(cleaned)
      if expected_abort:
        source_end = next(index + 1 for index, row in enumerate(source_rows) if row['op'] == 'ioctl' and row['ret'] != 0)
        native_end = next(index + 1 for index, row in enumerate(cleaned) if row['op'] == 'ioctl' and row['ret'] != 0)
        source_rows, cleaned = source_rows[:source_end], cleaned[:native_end]
        same_result = source[0]['exit'] == -6 and native[0]['exit'] == 1
      else:
        same_result = result_value(source[0]['stdout'], False) == result_value(native[0]['stdout'], True)
    except (AssertionError, KeyError, ValueError) as failure:
      error = str(failure)
      same_result = False
    successful_exit = expected_abort and same_result or source[0]['exit'] == native[0]['exit'] == 0
    passed = (successful_exit and not source[0]['sanitizer_failure']
              and not native[0]['sanitizer_failure'] and error is None and same_result and source_rows == cleaned)
    entry = dict(name=name, passed=passed, calls=len(source_rows), source_image_fds=source_ids, native_image_fds=native_ids, ownership=ownership,
                 native_resource_cleanup=resources, native_extra_cleanup=extra_cleanup, expected_source_abort=expected_abort)
    report['cases'].append(entry)
    if not passed:
      first = next(((index, left, right) for index, (left, right) in enumerate(zip(source_rows, cleaned, strict=False)) if left != right), None)
      report['failures'].append(dict(name=name, source=source[0], native=native[0], error=error,
          lengths=[len(source_rows), len(cleaned)], first_difference=first, equal_results=same_result))
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(name, 'PASS' if passed else 'FAIL', flush=True)
    if not passed: break
  print(json.dumps(dict(cases=len(report['cases']), failures=len(report['failures']))))
  raise SystemExit(bool(report['failures']))


if __name__ == '__main__':
  main()

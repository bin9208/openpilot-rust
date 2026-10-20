"""Cloud-only Panda timestamp path capture; never changes the acceptance assertion."""
from collections import Counter
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import shutil
import struct
import subprocess
import sys

from openpilot.cereal import log


def digest(path: Path) -> str:
  return sha256(path.read_bytes()).hexdigest()


def framed(path: Path, wrapped: bool) -> list[tuple[bytes, int, int]]:
  if not path.exists():
    return []
  raw = path.read_bytes()
  position = 0
  records = []
  while position < len(raw):
    if wrapped:
      if len(raw) - position < 24:
        raise ValueError(f'truncated wrapper header: {path}')
      length, result, error, flags, magic = struct.unpack_from('<QiiiI', raw, position)
      if magic != 0x50444c31 or flags != 1:
        raise ValueError(f'invalid wrapper record: {path}, {position}, {magic}, {flags}')
      position += 24
    else:
      if len(raw) - position < 8:
        raise ValueError(f'truncated ingress header: {path}')
      length, = struct.unpack_from('<Q', raw, position)
      result, error = -2, 0
      position += 8
    if length > 2 * 1024 * 1024 or length > len(raw) - position:
      raise ValueError(f'truncated or excessive record: {path}, {position}, {length}')
    records.append((raw[position:position + length], result, error))
    position += length
  return records


def timestamps(records: list[tuple[bytes, int, int]]) -> list[tuple[bytes, int, int]]:
  selected = []
  for body, result, error in records:
    if not body:
      raise ValueError('empty logging record')
    message = json.loads(body[1:])
    stamp = message.get('msg', {}).get('timestamp') if isinstance(message.get('msg'), dict) else None
    if stamp is not None and stamp['event'].startswith(('sending sendcan', 'sendcan sent')):
      selected.append((body[1:], result, error))
  return selected


def subscriber(path: Path) -> list[bytes]:
  if not path.exists():
    return []
  selected = []
  for event in log.Event.read_multiple_bytes(path.read_bytes()):
    if event.which() != 'logMessage':
      raise ValueError(f'wrong service in {path}')
    body = str(event.logMessage).encode()
    message = json.loads(body)
    stamp = message.get('msg', {}).get('timestamp') if isinstance(message.get('msg'), dict) else None
    if stamp is not None and stamp['event'].startswith(('sending sendcan', 'sendcan sent')):
      selected.append(body)
  return selected


def exercise(root: Path, output: Path, binary: Path, collector: Path, fixture: Path, name: str,
             instrumented: bool) -> dict:
  case = output / name
  environment = os.environ.copy()
  environment.pop('PANDA_DIAG_SEND', None)
  environment.pop('PANDA_DIAG_INGRESS', None)
  if instrumented:
    environment['PANDA_DIAG_SEND'] = str(case / 'sender.bin')
    environment['PANDA_DIAG_INGRESS'] = str(case / 'ingress.bin')
  case.mkdir(parents=True, exist_ok=False)
  command = [sys.executable, str(root / 'rust/tools/check_pandad_runtime.py'), '--original',
             '--case', 'onroad', '--binary', str(binary), '--collector', str(collector),
             '--fixture', str(fixture), '--output', str(case / 'check')]
  result = None
  timed_out = False
  with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
    process = subprocess.Popen(command, cwd=root, env=environment, stdout=stdout, stderr=stderr,
                               start_new_session=True)
    try:
      result = process.wait(timeout=45)
    except subprocess.TimeoutExpired:
      timed_out = True
      os.killpg(process.pid, signal.SIGKILL)
      process.wait(timeout=5)
  files = case / 'check/onroad'
  writes = []
  trace = files / 'usb-trace.jsonl'
  if trace.exists():
    writes = [row for line in trace.read_text().splitlines() if (row := json.loads(line))['op'] == 'write']
  expected = [[32, byte, 32, 0, 0, (byte + 38) & 255, 18, 52]
              for byte in (8, 16, 136, 144, 152)]
  write_match = [row['data'] for row in writes] == expected
  capture_errors = []
  sent = []
  ingress = []
  if instrumented:
    for label, path, wrapped in (('sender', case / 'sender.bin', True),
                                 ('ingress', case / 'ingress.bin', False)):
      if not path.is_file():
        capture_errors.append(f'{label} capture absent')
        continue
      try:
        captured = timestamps(framed(path, wrapped))
      except (ValueError, KeyError, UnicodeDecodeError) as error:
        capture_errors.append(f'{label} capture invalid: {error}')
        continue
      if label == 'sender':
        sent = captured
      else:
        ingress = captured
  received = subscriber(files / 'logMessage.bin')
  sender_bodies = Counter(body for body, _, _ in sent)
  ingress_bodies = Counter(body for body, _, _ in ingress)
  subscriber_bodies = Counter(received)
  failures = [(json.loads(body)['msg']['timestamp'], result, error)
              for body, result, error in sent if result < 0]
  send_events = [json.loads(body)['msg']['timestamp']['event'] for body, _, _ in sent]
  sender_complete = (len(sent) == 10 and all(
    event.startswith('sending sendcan' if index % 2 == 0 else 'sendcan sent')
    for index, event in enumerate(send_events)))
  row = {
    'name': name, 'instrumented': instrumented, 'returncode': result, 'timed_out': timed_out,
    'usb_write_count': len(writes), 'usb_writes_exact': write_match,
    'sender_timestamps': len(sent), 'ingress_timestamps': len(ingress),
    'subscriber_timestamps': len(received), 'send_failures': failures,
    'sender_capture_complete': sender_complete if instrumented else None,
    'capture_errors': capture_errors,
    'missing_at_ingress': [json.loads(body)['msg']['timestamp'] for body in (sender_bodies - ingress_bodies).elements()],
    'missing_at_subscriber': [json.loads(body)['msg']['timestamp'] for body in (ingress_bodies - subscriber_bodies).elements()],
    'unexpected_at_ingress': [json.loads(body)['msg']['timestamp'] for body in (ingress_bodies - sender_bodies).elements()],
    'unexpected_at_subscriber': [json.loads(body)['msg']['timestamp'] for body in (subscriber_bodies - ingress_bodies).elements()],
    'input_sha256': {str(path): digest(path) for path in
                     (files / 'usb-config.json', files / 'invocation.json', files / 'collector-onroad/readiness.json')
                     if path.is_file()},
  }
  (case / 'receipt.json').write_text(json.dumps(row, indent=2) + '\n')
  return row


def main() -> int:
  root, output, original, wrapped, original_collector, diagnostic_collector, fixture, manifest = map(
    lambda item: Path(item).resolve(), sys.argv[1:9])
  source_manifest = json.loads(manifest.read_text())
  tracked = [Path(path) for path in source_manifest['source_sha256']]
  tracked += [root / 'rust/tools/check_pandad_runtime.py', root / 'rust/tools/pandad_runtime_peer.py',
              root / 'rust/tools/logmessaged_native.py', root / 'openpilot/cereal/services.py',
              root / 'rust/Cargo.lock',
              Path(__file__).resolve(), Path(__file__).with_name('wrap_zmq_send.cc'),
              Path(__file__).with_name('make_collector_copy.py'),
              Path(__file__).with_name('rebuild_wrapped.py'),
              Path(sys.executable).resolve(), Path(shutil.which('clang++')).resolve(),
              Path(shutil.which('cargo')).resolve(),
              root / 'rust/target/debug/build-pandad-runtime-source',
              root / 'rust/target/debug/build-pandad-runtime-fixture',
              original, wrapped,
              original_collector, diagnostic_collector, fixture]
  tracked += [root / 'rust/crates/logmessaged/src/bin/panda-diag-logmessaged.rs']
  import msgq.ipc_pyx
  import usb1
  tracked += [Path(msgq.ipc_pyx.__file__).resolve(), Path(usb1.__file__).resolve()]
  before = {str(path): digest(path) for path in tracked}
  assert all(before[path] == expected for path, expected in source_manifest['source_sha256'].items())
  (output / 'inputs.json').write_text(json.dumps(before, indent=2) + '\n')
  rows = []
  # The first group preserves the original unmodified binary and collector.
  # Every run remains a fresh owned prefix and a bounded original checker invocation.
  for index in range(8):
    rows.append(exercise(root, output, original, original_collector, fixture, f'control-{index}', False))
  for index in range(8):
    rows.append(exercise(root, output, wrapped, diagnostic_collector, fixture, f'instrumented-{index}', True))
  after = {str(path): digest(path) for path in tracked}
  assert before == after, 'a source, helper, binary or fixture changed during diagnostic runs'
  (output / 'inputs-after.json').write_text(json.dumps(after, indent=2) + '\n')
  (output / 'results.json').write_text(json.dumps(rows, indent=2) + '\n')
  losses = [row for row in rows if row['instrumented'] and row['usb_writes_exact']
            and row['subscriber_timestamps'] != 10]
  if not losses:
    print('UNINFORMATIVE: no instrumented run reproduced a missing timestamp', flush=True)
    return 2
  complete = [row for row in losses if row['sender_capture_complete'] and not row['capture_errors']
              and not row['timed_out']]
  if not complete:
    print('INCONCLUSIVE: reproduced subscriber loss without complete 10-attempt sender and framed ingress captures', flush=True)
    return 3
  print(json.dumps({'result': 'DIAGNOSTIC_REPRODUCED', 'complete_losses': complete,
                    'inconclusive_losses': [row for row in losses if row not in complete]}, indent=2), flush=True)
  return 0


if __name__ == '__main__':
  sys.exit(main())

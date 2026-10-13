from __future__ import annotations

import argparse
from collections import deque
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time
from typing import Final


ROOT: Final = Path(__file__).resolve().parents[2]


@dataclass(frozen=True, slots=True)
class Scenario:
  name: str
  action: str
  gate: str | None
  replies: tuple[tuple[int, str], ...]
  dongle: str = 'owned-device'
  serial: str = 'owned-hardware'
  expected_gate: str | None = None
  expected_requests: int = 1


SCENARIOS: Final = (
  Scenario('ready_preserves_enable', 'status', '1', ((200, '{"ok":true,"registered":true}'),), expected_gate='1'),
  Scenario('lost_registration_clears_alias', 'status', '1', ((200, '{"ok":true,"registered":false}'),), expected_gate='0'),
  Scenario('registered_enable_persists', 'enable', '0', ((200, '{"ok":true,"registered":true}'),), expected_gate='1'),
  Scenario('unregistered_enable_rejected', 'enable', '1', ((200, '{"ok":true,"registered":false}'),), expected_gate='0'),
  Scenario('unavailable_enable_rejected', 'enable', '0', ((503, 'owned service unavailable'),), expected_gate='0'),
  Scenario('unavailable_disable_persists', 'disable', '1', ((503, 'owned service unavailable'),), expected_gate='0'),
  Scenario('invalid_json_preserves_enable', 'status', '1', ((200, 'broken-json'),), expected_gate='1'),
  Scenario('nonobject_response_preserves_enable', 'status', '1', ((200, '[true]'),), expected_gate='1'),
  Scenario('application_rejection_preserves_enable', 'status', '1', ((200, '{"ok":false,"registered":false}'),), expected_gate='1'),
  Scenario('serial_sentinel_fallback', 'status', 'true', ((200, '{"ok":1,"registered":"yes"}'),), dongle=' UNREGISTEREDDEVICE ', serial=' owned-hardware \r\n', expected_gate='true'),
  Scenario('hostname_sentinel_fallback', 'status', None, ((200, '{"ok":true,"registered":true}'),), dongle='null', serial='UNKNOWN'),
  Scenario('boot_disabled_gate', 'boot', '0', (), expected_gate='0', expected_requests=0),
  Scenario('boot_registered_preserves_enable', 'boot', '1', ((200, '{"ok":true,"registered":true,"pushed":3}'),), expected_gate='1'),
  Scenario('boot_retry_then_unregister', 'boot', '1', ((503, 'transient'), (200, '{"ok":true,"registered":false,"pushed":"2"}')), expected_gate='0', expected_requests=2),
)


def run_scenario(scenario: Scenario, implementation: str, binary: Path, output: Path) -> tuple[str, list[dict[str, str]], float]:
  fixture = output / scenario.name / implementation
  alias = fixture / 'params' / 'alias'
  alias.mkdir(parents=True)
  (fixture / 'params' / 'd').symlink_to('alias', target_is_directory=True)
  (alias / 'DongleId').write_text(scenario.dongle)
  (alias / 'HardwareSerial').write_text(scenario.serial)
  if scenario.gate is not None:
    (alias / 'CwebPushRecoveryBoot').write_text(scenario.gate)
  (fixture / 'ip').write_text('192.168.42.9')
  replies = deque(scenario.replies)
  captured: list[dict[str, str]] = []
  started = time.monotonic()

  class Peer(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
      payload = self.rfile.read(int(self.headers['Content-Length']))
      captured.append({
        'method': self.command,
        'path': self.path,
        'payload': payload.decode(),
        'accept': self.headers.get('Accept', ''),
        'content_type': self.headers.get('Content-Type', ''),
        'user_agent': self.headers.get('User-Agent', ''),
        'authorization': self.headers.get('Authorization', ''),
        'elapsed': str(time.monotonic() - started),
      })
      status, text = replies.popleft()
      body = text.encode()
      self.send_response(status)
      self.send_header('Content-Type', 'application/json')
      self.send_header('Content-Length', str(len(body)))
      self.end_headers()
      self.wfile.write(body)

    def log_message(self, format: str, *args: str) -> None:
      return

  with ThreadingHTTPServer(('127.0.0.1', 0), Peer) as peer:
    worker = threading.Thread(target=peer.serve_forever)
    worker.start()
    environment = os.environ.copy()
    environment.update({
      'CWP_PROBE_ROOT': str(fixture),
      'CWP_PROBE_ACTION': scenario.action,
      'CWEB_PUSH_REPORT_URL': f'http://127.0.0.1:{peer.server_port}/report',
      'CWP_REPORT_TOKEN': ' owned-loopback-token ',
      'PYTHONPYCACHEPREFIX': str(output / 'pycache'),
    })
    match implementation:
      case 'rust':
        command = [str(binary), '--exact', 'cwp::tests::child_driver', '--ignored', '--nocapture']
      case 'source':
        command = [sys.executable, str(ROOT / 'rust/tools/carrot_recovery_cwp_source.py')]
      case other:
        raise RuntimeError(f'unknown implementation: {other}')
    try:
      process = subprocess.run(command, env=environment, cwd=ROOT, capture_output=True, text=True, timeout=20, check=False)
      elapsed = time.monotonic() - started
    finally:
      peer.shutdown()
      worker.join()
  (fixture / 'stdout.txt').write_text(process.stdout)
  (fixture / 'stderr.txt').write_text(process.stderr)
  (fixture / 'requests.json').write_text(json.dumps(captured, indent=2) + '\n')
  (fixture / 'invocation.json').write_text(json.dumps({'command': command, 'exit_code': process.returncode, 'elapsed': elapsed}, indent=2) + '\n')
  if process.returncode != 0:
    raise RuntimeError(f'{scenario.name}/{implementation} exited {process.returncode}: {process.stderr}')
  result = next(line.removeprefix('CWP_RESULT=') for line in process.stdout.splitlines() if line.startswith('CWP_RESULT='))
  (fixture / 'result.json').write_text(result + '\n')
  gate_path = alias / 'CwebPushRecoveryBoot'
  gate = gate_path.read_text() if gate_path.is_file() else None
  assert gate == scenario.expected_gate, (scenario.name, implementation, gate, scenario.expected_gate)
  assert len(captured) == scenario.expected_requests, (scenario.name, implementation, captured)
  assert (fixture / 'params' / 'd').is_symlink()
  assert not list(alias.glob('.tmp_*'))
  for request in captured:
    assert request['path'] == '/recovery/' + ('boot' if scenario.action == 'boot' else 'status')
    assert request['accept'] == 'application/json'
    assert request['content_type'] == 'application/json'
    assert request['user_agent'] == 'CarrotRecovery/2.0'
    assert request['authorization'] == 'Bearer owned-loopback-token'
  if scenario.action == 'boot' and captured:
    assert float(captured[0]['elapsed']) >= 2.0
    if len(captured) > 1:
      assert float(captured[1]['elapsed']) - float(captured[0]['elapsed']) >= 5.0
  return result, captured, elapsed


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--test-binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  binary = args.test_binary.resolve()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  results: list[dict[str, str | int | bool]] = []
  for scenario in SCENARIOS:
    source, source_requests, source_time = run_scenario(scenario, 'source', binary, output)
    rust, rust_requests, rust_time = run_scenario(scenario, 'rust', binary, output)
    assert json.loads(source) == json.loads(rust), (scenario.name, source, rust)
    assert [json.loads(request['payload']) for request in source_requests] == [json.loads(request['payload']) for request in rust_requests]
    if scenario.name == 'hostname_sentinel_fallback':
      assert json.loads(rust_requests[0]['payload'])['deviceId'] == socket.gethostname()
    results.append({'scenario': scenario.name, 'pass': True, 'requests_each': len(rust_requests), 'source_seconds': str(source_time), 'rust_seconds': str(rust_time), 'artifact': str(output / scenario.name)})
  (output / 'summary.json').write_text(json.dumps({'pass': True, 'scenarios': results}, indent=2) + '\n')
  print(json.dumps({'pass': True, 'scenario_count': len(results), 'artifact': str(output / 'summary.json')}))


if __name__ == '__main__':
  main()

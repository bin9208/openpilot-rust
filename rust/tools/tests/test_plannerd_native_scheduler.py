"""실제 run의 stdio adapter를 AST로 로드한다. C++/모델 import 없이 Actions에서 실행한다."""
import ast
import hashlib
import json
import math
import os
from pathlib import Path
import select
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / 'plannerd_ipc.py'
ORACLE_SHA256 = 'a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1'
PEER_SOURCE = r'''
import os, signal, sys, time
assert sys.argv[1:] == ['schedule']
mode = os.environ['PLANNER_TEST_MODE']
if mode == 'ignore-term':
  signal.signal(signal.SIGTERM, signal.SIG_IGN)
if mode == 'eof':
  sys.exit(0)
if mode == 'exit42':
  sys.exit(42)
for line in sys.stdin:
  assert len(line.split()) == 3
  with open(os.environ['PLANNER_TEST_REQUESTS'], 'a') as log:
    log.write(line)
  if mode == 'timeout':
    time.sleep(30)
  reply = {'malformed': 'invalid', 'nonfinite': 'nan', 'long': 'x' * 300,
           'multiple': '0.0\n1.0'}.get(mode, '0.0')
  # multiple은 한 write의 두 행이다.
  sys.stdout.write(reply + '\n')
  sys.stdout.flush()
if mode == 'close42':
  sys.exit(42)
if mode == 'ignore-term':
  time.sleep(30)
'''


def load_scheduler():
  tree = ast.parse(SOURCE.read_bytes())
  namespace = dict(__name__='planner_scheduler_source', math=math, os=os, select=select,
                   subprocess=subprocess, sys=sys, time=time)
  nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef))
           and node.name in ('next_model_target', 'ModelScheduler')]
  if len(nodes) != 2:
    raise AssertionError('actual IPC scheduler and independent oracle are required')
  exec(compile(ast.Module(nodes, type_ignores=[]), str(SOURCE), 'exec'), namespace)
  return namespace


class NativeSchedulerTests(unittest.TestCase):
  def setUp(self):
    self.temp = tempfile.TemporaryDirectory(prefix='planner-scheduler-')
    self.addCleanup(self.temp.cleanup)
    self.root = Path(self.temp.name)
    peer = self.root / 'peer.py'
    peer.write_text(PEER_SOURCE)
    self.binary = self.root / 'native-scheduler'
    self.binary.write_text('#!/bin/sh\nexec "$PLANNER_TEST_PYTHON" -u "$PLANNER_TEST_PEER" "$@"\n')
    self.binary.chmod(0o755)
    self.requests = self.root / 'requests.tsv'
    self.environment = patch.dict(os.environ, PLANNER_NATIVE_SCHEDULER=str(self.binary),
                                  PLANNER_TEST_PYTHON=sys.executable, PLANNER_TEST_PEER=str(peer),
                                  PLANNER_TEST_REQUESTS=str(self.requests), PLANNER_TEST_MODE='normal')
    self.environment.start()
    self.addCleanup(self.environment.stop)
    self.namespace = load_scheduler()
    self.Scheduler = self.namespace['ModelScheduler']

  def peer(self):
    return self.Scheduler(self.root / 'scheduler.stderr.log')

  def forbidden_oracle(self):
    return patch.dict(self.namespace, next_model_target=lambda *_: self.fail('native error fell back to the oracle'))

  def test_independent_oracle_bytes_are_unchanged(self):
    raw = SOURCE.read_bytes()
    node = next(node for node in ast.parse(raw).body if isinstance(node, ast.FunctionDef) and node.name == 'next_model_target')
    function = b''.join(raw.splitlines(keepends=True)[node.lineno - 1:node.end_lineno])
    self.assertEqual(hashlib.sha256(function).hexdigest(), ORACLE_SHA256)

  def test_actual_run_loop_uses_one_scheduler_context_and_one_request_per_frame(self):
    run = next(node for node in ast.parse(SOURCE.read_bytes()).body if isinstance(node, ast.FunctionDef) and node.name == 'run')
    contexts = [item for node in ast.walk(run) if isinstance(node, ast.With) for item in node.items
                if isinstance(item.context_expr, ast.Call) and isinstance(item.context_expr.func, ast.Name)
                and item.context_expr.func.id == 'ModelScheduler']
    self.assertEqual(len(contexts), 1)
    self.assertEqual(contexts[0].optional_vars.id, 'scheduler')
    loop = next(node for node in ast.walk(run) if isinstance(node, ast.For) and isinstance(node.target, ast.Name) and node.target.id == 'index')
    requests = [node for node in ast.walk(loop) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and isinstance(node.func.value, ast.Name) and node.func.value.id == 'scheduler' and node.func.attr == 'target']
    self.assertEqual(len(requests), 1)
    self.assertFalse(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'next_model_target'
                         for node in ast.walk(run)))

  def test_absent_environment_preserves_oracle_signed_zero_and_nan_without_spawning(self):
    os.environ.pop('PLANNER_NATIVE_SCHEDULER')
    oracle = self.namespace['next_model_target']
    with patch.object(subprocess, 'Popen') as spawn:
      with self.peer() as peer:
        for case in ((-0.0, -0.05, -0.05), (float('nan'), 100.0, 100.08)):
          self.assertEqual(struct.pack('>d', peer.target(*case)), struct.pack('>d', oracle(*case)))
      spawn.assert_not_called()
    self.assertFalse((self.root / 'scheduler.stderr.log').exists())

  def test_persistent_process_serves_forty_requests_and_is_reaped_on_eof(self):
    real_spawn = subprocess.Popen
    with self.forbidden_oracle(), patch.object(subprocess, 'Popen', wraps=real_spawn) as spawn:
      with self.peer() as peer:
        for index in range(40):
          self.assertEqual(peer.target(100.05 + index * 0.05, 100.0, 100.003), 0.0)
      self.assertEqual(spawn.call_count, 1)
      self.assertEqual(peer.process.returncode, 0)
      self.assertTrue(peer.process.stdin.closed and peer.process.stdout.closed)
    self.assertEqual(len(self.requests.read_text().splitlines()), 40)

  def test_empty_setting_is_an_error_instead_of_oracle_mode(self):
    os.environ['PLANNER_NATIVE_SCHEDULER'] = ''
    with self.forbidden_oracle(), patch.object(subprocess, 'Popen') as spawn:
      with self.assertRaises(RuntimeError):
        with self.peer():
          self.fail('empty native setting was accepted')
      spawn.assert_not_called()

  def test_spawn_failure_does_not_fall_back(self):
    os.environ['PLANNER_NATIVE_SCHEDULER'] = str(self.root / 'missing')
    with self.forbidden_oracle(), self.assertRaises(FileNotFoundError):
      with self.peer():
        self.fail('missing native executable was accepted')

  def test_eof_exit_and_invalid_response_fail_closed_and_reap(self):
    for mode in ('eof', 'exit42', 'malformed', 'nonfinite', 'long', 'multiple'):
      with self.subTest(mode=mode):
        os.environ['PLANNER_TEST_MODE'] = mode
        peer = self.peer()
        with self.forbidden_oracle(), self.assertRaises((RuntimeError, OSError, ValueError)):
          with peer:
            peer.target(100.05, 100.0, 100.003)
        self.assertTrue(peer.failed)
        self.assertIsNotNone(peer.process.returncode)

  def test_response_timeout_fails_closed_and_reaps(self):
    os.environ['PLANNER_TEST_MODE'] = 'timeout'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaises(TimeoutError):
      with peer:
        peer.target(100.05, 100.0, 100.003)
    self.assertIsNotNone(peer.process.returncode)

  def test_caught_eof_does_not_allow_retry_or_successful_context_exit(self):
    os.environ['PLANNER_TEST_MODE'] = 'eof'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaises(RuntimeError):
      with peer:
        with self.assertRaises((RuntimeError, OSError)):
          peer.target(100.05, 100.0, 100.003)
        with self.assertRaises(RuntimeError):
          peer.target(100.05, 100.0, 100.003)
    self.assertEqual(peer.process.returncode, 0)

  def test_abnormal_close_status_is_not_silently_accepted(self):
    os.environ['PLANNER_TEST_MODE'] = 'close42'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaisesRegex(RuntimeError, '42'):
      with peer:
        self.assertEqual(peer.target(100.05, 100.0, 100.003), 0.0)
    self.assertEqual(peer.process.returncode, 42)

  def test_normal_close_escalates_term_kill_wait_and_reports_failure(self):
    os.environ['PLANNER_TEST_MODE'] = 'ignore-term'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaises(RuntimeError):
      with peer:
        self.assertEqual(peer.target(100.05, 100.0, 100.003), 0.0)
    self.assertEqual(peer.process.returncode, -signal.SIGKILL)

  def test_exception_cleanup_reaps_stubborn_child_and_preserves_parent_error(self):
    os.environ['PLANNER_TEST_MODE'] = 'ignore-term'
    peer = self.peer()
    with self.forbidden_oracle(), self.assertRaisesRegex(ValueError, 'parent run failed'):
      with peer:
        self.assertEqual(peer.target(100.05, 100.0, 100.003), 0.0)
        raise ValueError('parent run failed')
    self.assertEqual(peer.process.returncode, -signal.SIGKILL)

  @unittest.skipUnless(os.environ.get('PLANNER_NATIVE_SCHEDULER_TEST_BIN'), 'Actions native binary registration required')
  def test_real_native_binary_receives_all_160_calls_with_matching_targets(self):
    os.environ['PLANNER_NATIVE_SCHEDULER'] = os.environ['PLANNER_NATIVE_SCHEDULER_TEST_BIN']
    evidence = self.root / 'native-scheduler.tsv'
    os.environ['PLANNER_NATIVE_SCHEDULER_EVIDENCE'] = str(evidence)
    oracle = self.namespace['next_model_target']
    real_spawn = subprocess.Popen
    with patch.object(subprocess, 'Popen', wraps=real_spawn) as spawn:
      for run in range(4):
        with self.Scheduler(self.root / f'scheduler-{run}.stderr.log') as peer:
          for index in range(40):
            boundaries = ((-0.0, -0.05, -0.05), (0.0, -0.05, -0.05),
                          (-0.0, -0.1, -0.05), (0.0, -0.1, -0.05))
            case = boundaries[index] if index < len(boundaries) else (
              100.05 + index * 0.05, 100.0 + index * 0.05, 100.003 + index * 0.05,
            )
            self.assertEqual(struct.pack('>d', peer.target(*case)), struct.pack('>d', oracle(*case)))
        self.assertEqual(peer.process.returncode, 0)
      self.assertEqual(spawn.call_count, 4)
    rows = evidence.read_text().splitlines()
    self.assertEqual(len(rows), 160)
    for row in rows:
      nominal, sent, done, target = map(float, row.split())
      self.assertEqual(struct.pack('>d', target), struct.pack('>d', oracle(nominal, sent, done)))


class CaptureCollectionTests(unittest.TestCase):
  def test_capture_preserves_single_serialization_four_runs_bounds_and_original_parser_stage(self):
    raw = SOURCE.read_bytes()
    tree = ast.parse(raw)
    collector = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'InputCapture')
    fixtures = ast.parse((SOURCE.parent / 'plannerd_owner_fixtures.py').read_bytes())
    services = ast.literal_eval(next(node.value for node in fixtures.body if isinstance(node, ast.Assign)
                                    and any(isinstance(target, ast.Name) and target.id == 'SERVICES' for target in node.targets)))
    namespace = dict(__name__='planner_input_capture_source', __file__=str(SOURCE), os=os, stat=stat,
                     Path=Path, hashlib=hashlib, json=json, sys=sys, SERVICES=services)
    exec(compile(ast.Module([collector], type_ignores=[]), str(SOURCE), 'exec'), namespace)
    Capture = namespace['InputCapture']

    class Packet:
      def __init__(self, payload):
        self.payload, self.serializations = payload, 0

      def to_bytes(self):
        self.serializations += 1
        return self.payload

    class Publisher:
      def __init__(self):
        self.calls = []

      def send(self, service, payload):
        if not isinstance(payload, bytes):
          raise AssertionError('collector must pass the same serialized bytes')
        self.calls.append((service, payload))

    with tempfile.TemporaryDirectory(prefix='planner-collector-') as temporary:
      root = Path(temporary)
      with patch.dict(os.environ):
        os.environ.pop('PLANNER_IPC_CAPTURE_DIR', None)
        self.assertIsNone(Capture.from_environment(40))
        for invalid in ('', '0', 'relative-capture'):
          os.environ['PLANNER_IPC_CAPTURE_DIR'] = invalid
          with self.assertRaises(ValueError):
            Capture.from_environment(40)
      # 이 임시자료는 수집기 unit fixture다. 실제 IPC 입력 corpus/수용 근거로 업로드하지 않는다.
      capture = Capture(root / 'capture', '0' * 40, {}, {'input_transport': 'unit-fake'})
      order = [name for name in services if name not in ('liveTracks', 'modelV2')] + ['modelV2']
      for mode, epoch in (('source', 0), ('source', 1), ('rust', 0), ('rust', 1)):
        params = root / f'params-{mode}-{epoch}'
        params.mkdir()
        (params / 'EnableRadarTracks').write_bytes(b'1')
        run = capture.begin_run(mode, epoch, f'prefix-{mode}-{epoch}', params, ['unit-fixture'])
        (params / 'EnableRadarTracks').write_bytes(b'changed-after-snapshot')
        pending = params / 'CarParams.pending'
        pending.write_bytes(f'actual pending {mode}-{epoch}'.encode())
        run.pending(pending)
        publisher = Publisher()
        packets = []

        def send(service, index=None):
          packet = Packet(f'{mode}:{epoch}:{len(packets)}:{service}'.encode())
          packets.append(packet)
          run.send(publisher, service, packet, index)
          self.assertIs(publisher.calls[-1][1], packet.payload)

        send('liveTracks')
        for index in range(40):
          for service in order:
            send(service, index)
          send('liveTracks', index)
        run.finish()
        self.assertEqual(len(publisher.calls), 401)
        self.assertTrue(all(packet.serializations == 1 for packet in packets))
        manifest = json.loads((run.directory / 'manifest.json').read_bytes())
        self.assertEqual(len(manifest['frames']), 40)
        self.assertEqual((run.directory / manifest['params']['EnableRadarTracks']['path']).read_bytes(), b'1')
        self.assertEqual((run.directory / manifest['car_params']['path']).read_bytes(), pending.read_bytes())
        self.assertEqual(manifest['capture_provenance']['mode'], mode)
        self.assertEqual(manifest['capture_provenance']['epoch'], str(epoch))
        captured = [manifest['initial_live']]
        for frame in manifest['frames']:
          self.assertEqual([packet['service'] for packet in frame['packets']], order)
          captured.extend(packet['raw'] for packet in frame['packets'])
          captured.append(frame['live'])
        for raw_file, (_, payload) in zip(captured, publisher.calls, strict=True):
          self.assertEqual((run.directory / raw_file['path']).read_bytes(), payload)
          self.assertEqual(raw_file['sha256'], hashlib.sha256(payload).hexdigest())
        with self.assertRaises(RuntimeError):
          run.finish()
      output_receipt = root / 'actual-receipt.json'
      output_receipt.write_bytes(b'{"scope":"unit-fake"}\n')
      capture.finish(output_receipt)
      index = json.loads((capture.directory / 'index.json').read_bytes())
      self.assertEqual((index['events'], index['frames'], index['car_params_files']), (1604, 160, 4))
      self.assertEqual(len(index['runs']), 4)
      self.assertEqual(len({run['manifest']['sha256'] for run in index['runs']}), 4)
      for path in (capture.directory, *capture.directory.rglob('*')):
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o700 if path.is_dir() else 0o600)
      with self.assertRaises(FileExistsError):
        Capture(capture.directory, '0' * 40, {}, {})
      with self.assertRaises(FileExistsError):
        capture.finish(output_receipt)
      bounded = Capture(root / 'bounded', '0' * 40, {}, {'input_transport': 'unit-fake'})
      bounded.TOTAL_LIMIT = 1
      with self.assertRaises(ValueError):
        bounded._write(bounded.directory, 'too-large.bin', b'12', 2)
      self.assertTrue(bounded.failed)
      self.assertFalse((bounded.directory / 'too-large.bin').exists())
      empty_params = root / 'empty-params'
      empty_params.mkdir()
      for label, packet_limit, run_limit in (('packet-bound', 1, 64), ('run-bound', 64, 1)):
        limited = Capture(root / label, '0' * 40, {}, {'input_transport': 'unit-fake'})
        limited.PACKET_LIMIT, limited.RUN_LIMIT = packet_limit, run_limit
        run = limited.begin_run('source', 0, 'unit-prefix', empty_params, ['unit-fixture'])
        pending = root / f'{label}.pending'
        pending.write_bytes(b'1')
        run.pending(pending)
        publisher = Publisher()
        packet = Packet(b'12')
        with self.assertRaises(ValueError):
          run.send(publisher, 'liveTracks', packet)
        self.assertTrue(limited.failed)
        self.assertEqual(packet.serializations, 1)
        self.assertEqual(publisher.calls, [])
        self.assertEqual(list((run.directory / 'events').iterdir()), [])
      snapshot_params = root / 'snapshot-bounds'
      snapshot_params.mkdir()
      (snapshot_params / 'First').write_bytes(b'12')
      (snapshot_params / 'Second').write_bytes(b'34')
      for label, key_limit, snapshot_limit in (('key-bound', 1, 4), ('snapshot-bound', 2, 3)):
        limited = Capture(root / label, '0' * 40, {}, {'input_transport': 'unit-fake'})
        limited.PARAM_LIMIT, limited.PARAMS_LIMIT = key_limit, snapshot_limit
        with self.assertRaises(ValueError):
          limited.begin_run('source', 0, 'unit-prefix', snapshot_params, ['unit-fixture'])
        self.assertFalse((limited.directory / 'index.json').exists())
      link_params = root / 'symlink-params'
      link_params.mkdir()
      (link_params / 'BadValue').symlink_to(output_receipt)
      untrusted = Capture(root / 'untrusted', '0' * 40, {}, {'input_transport': 'unit-fake'})
      with self.assertRaises(OSError):
        untrusted.begin_run('source', 0, 'unit-prefix', link_params, ['unit-fixture'])

    scheduler = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'ModelScheduler')
    target = next(node for node in scheduler.body if isinstance(node, ast.FunctionDef) and node.name == 'target')
    body = next(node for node in target.body if isinstance(node, ast.Try)).body
    conversion = next(node for node in body if isinstance(node, ast.Assign)
                      and any(isinstance(name, ast.Name) and name.id == 'target' for name in node.targets))
    finite = next(node for node in body if isinstance(node, ast.If) and ast.unparse(node.test) == 'not math.isfinite(target)')
    returned = next(node for node in body if isinstance(node, ast.Return) and isinstance(node.value, ast.Name) and node.value.id == 'target')
    stage = ast.Module([conversion, finite, returned], type_ignores=[])
    expected = ast.parse("target = float(line.decode('ascii'))\nif not math.isfinite(target):\n  raise RuntimeError('native scheduler returned a nonfinite target')\nreturn target\n")
    self.assertEqual(ast.dump(stage, include_attributes=False), ast.dump(expected, include_attributes=False))
    function = ast.FunctionDef(name='original_parser_stage',
                               args=ast.arguments(posonlyargs=[], args=[ast.arg(arg='line')], vararg=None,
                                                  kwonlyargs=[], kw_defaults=[], kwarg=None, defaults=[]),
                               body=stage.body, decorator_list=[])
    parser_namespace = {'math': math}
    exec(compile(ast.fix_missing_locations(ast.Module([function], type_ignores=[])), str(SOURCE), 'exec'), parser_namespace)
    cases = (
      ('zero', b'0.0'), ('negative-zero', b'-0.0'), ('positive-sign', b'+0.0'), ('integer', b'1'),
      ('space', b' 1.25 '), ('tab', b'\t1.25\t'), ('vertical-tab', b'\v1.25\f'), ('carriage-return', b'1.25\r'),
      ('exponent', b'1.25e+2'), ('negative-exponent', b'-1.25E-2'), ('underscore', b'1_0.0'),
      ('underscore-exponent', b'1e1_0'), ('leading-dot', b'.5'), ('trailing-dot', b'5.'),
      ('maximum-finite', b'1.7976931348623157e308'), ('overflow', b'1.7976931348623159e308'),
      ('large-overflow', b'1e309'), ('negative-overflow', b'-1e309'), ('underflow', b'1e-4000'),
      ('negative-underflow', b'-1e-4000'), ('nan', b'nan'), ('signed-nan', b'-NaN'),
      ('inf', b'inf'), ('infinity', b'+Infinity'), ('empty', b''), ('whitespace-only', b' \t'),
      ('invalid', b'invalid'), ('double-underscore', b'1__0'), ('leading-underscore', b'_1'),
      ('trailing-underscore', b'1_'), ('hex-float', b'0x1p0'), ('nul', b'1\x00'),
      ('nonascii', b'\xff'), ('utf8-space', b'\xc2\xa01.0'), ('two-lines-stage', b'0.0\n1.0'),
    )
    observations = []
    for name, line in cases:
      self.assertLessEqual(len(line), 255)
      row = {'name': name, 'line_hex': line.hex(), 'line_type': 'bytearray'}
      try:
        result = parser_namespace['original_parser_stage'](bytearray(line))
        row.update(result_bits_be_hex=struct.pack('>d', result).hex(), exception_type=None, exception_message=None)
      except (ValueError, UnicodeDecodeError, RuntimeError) as error:
        row.update(result_bits_be_hex=None, exception_type=type(error).__name__, exception_message=str(error))
      observations.append(row)
    self.assertEqual(len(observations), 35)
    self.assertEqual(observations[1]['result_bits_be_hex'], '8000000000000000')
    scheduler_bytes = b''.join(raw.splitlines(keepends=True)[scheduler.lineno - 1:scheduler.end_lineno])
    observation = {
      'schema_version': 1, 'kind': 'independent-original-planner-parser-stage',
      'scope': 'exact float(ASCII decode)+finite AST stage only; not full client, framing, latch or lifecycle acceptance',
      'github_sha': os.environ.get('GITHUB_SHA'), 'source_sha256': hashlib.sha256(raw).hexdigest(),
      'scheduler_class_sha256': hashlib.sha256(scheduler_bytes).hexdigest(),
      'test_source_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
      'stage_ast_sha256': hashlib.sha256(ast.dump(stage, include_attributes=False).encode()).hexdigest(),
      'python_executable': sys.executable, 'python_executable_sha256': Capture._sha(Path(sys.executable).resolve(strict=True)),
      'python_version': sys.version, 'cases': observations,
    }
    Capture.record_parser_observations(observation)


if __name__ == '__main__':
  unittest.main()

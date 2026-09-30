#!/usr/bin/env python3
import argparse
import ast
import asyncio
from collections import deque
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
from typing import Any
from urllib.parse import unquote
import uuid

from aiohttp import web, ClientSession, ClientTimeout


def module(path, name):
  spec = importlib.util.spec_from_file_location(name, path)
  value = importlib.util.module_from_spec(spec)
  sys.modules[name] = value
  spec.loader.exec_module(value)
  return value


def definitions(path, scope):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Assign, ast.AnnAssign))]
  exec(compile(tree, str(path), 'exec'), scope)


def map_commit_repository(report):
  original_lines = report.upload_message_lines

  def lines(*args, **kwargs):
    return [
      line.replace('https://github.com/ajouatom/openpilot/commit/', 'https://github.com/bin9208/openpilot-rust/commit/')
      for line in original_lines(*args, **kwargs)
    ]

  report.upload_message_lines = lines
  return report


def source_scope():
  root = Path(__file__).resolve().parents[2]
  feature = root / 'openpilot/selfdrive/carrot/server/features/dashcam'
  report = map_commit_repository(module(root / 'openpilot/selfdrive/carrot/server/services/dashcam_upload_report.py', 'reference_report'))
  transport = module(root / 'openpilot/selfdrive/carrot/web_upload.py', 'reference_transport')
  config = {}
  scope = {
    'os': os,
    'web': web,
    'Any': Any,
    're': re,
    'threading': threading,
    'RLOG_SOURCE_NAMES': ('rlog.zst', 'rlog.bz2', 'rlog'),
    'UPLOAD_SOURCE_GROUPS': (('qcamera', ('qcamera.ts', 'qcamera.mp4')), ('rlog', ('rlog.zst', 'rlog.bz2', 'rlog'))),
  }
  definitions(feature / 'paths.py', scope)
  definitions(feature / 'catalog.py', scope)
  upload_scope = {'Any': Any, 'ClientSession': ClientSession, 'ClientTimeout': ClientTimeout, 'discord_content': report.discord_content}
  tree = ast.parse((feature / 'upload.py').read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'send_discord_webhook']
  exec(compile(tree, str(feature / 'upload.py'), 'exec'), upload_scope)
  scope.update(
    asyncio=asyncio,
    time=time,
    uuid=uuid,
    deque=deque,
    datetime=datetime,
    HAS_PARAMS=False,
    create_web_upload_session=transport.create_web_upload_session,
    send_web_upload_complete=transport.send_web_upload_complete,
    upload_device_id=transport.upload_device_id,
    upload_folder_to_web=transport.upload_folder_to_web,
    upload=SimpleNamespace(
      upload_target_settings=lambda: (config['base_url'], config['token']),
      upload_metadata=lambda params: config['metadata'],
      upload_share_text=report.upload_share_text,
      discord_webhook_url=lambda params: config['webhook'],
      send_discord_webhook=upload_scope['send_discord_webhook'],
    ),
  )
  definitions(feature / 'upload_jobs.py', scope)
  return scope, config


async def source_peer(scope, config):
  while line := await asyncio.to_thread(sys.stdin.readline):
    request = json.loads(line)
    try:
      op = request['op']
      if op == 'start':
        scope['DASHCAM_ROOT'] = request['root']
        segments = [scope['safe_segment'](segment) for segment in request['segments']]
        if not segments:
          raise web.HTTPBadRequest(text='missing segments')
        incomplete = [segment for segment in segments if not scope['segment_is_complete'](segment)]
        if incomplete:
          raise web.HTTPConflict(text=f'segment is still recording or incomplete: {incomplete[0]}')
        running = scope['running_job']()
        if running:
          result = {'ok': False, 'error': 'upload already running', 'job_id': running['id'], 'job': scope['snapshot'](running)}
        else:
          config.clear()
          config.update(request['settings'])
          os.environ['CARROT_WEB_UPLOAD_CONCURRENCY'] = str(config['concurrency'])
          job = scope['create_job'](segments)
          scope['start_job'](job)
          result = {'ok': True, 'job_id': job['id'], 'status': job['status']}
      elif op == 'snapshot':
        scope['expire_stale_jobs']()
        job = scope['_jobs'].get(request['id'])
        result = scope['snapshot'](job) if job else {'ok': False, 'error': 'job not found'}
      elif op == 'cancel':
        result = scope['cancel_job'](request['id'])
      elif op == 'expire':
        scope['expire_stale_jobs'](request.get('now'))
        result = {'ok': True}
      elif op == 'jobs':
        result = [scope['snapshot'](job) for job in scope['_jobs'].values()]
      else:
        raise ValueError(op)
    except web.HTTPException as error:
      result = {'ok': False, 'error': error.text}
    except Exception as error:
      result = {'ok': False, 'error': str(error)}
    print(json.dumps(result), flush=True)


class Peer:
  def __init__(self, command, log):
    self.log = log.open('w')
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, text=True)

  def call(self, **request):
    self.process.stdin.write(json.dumps(request) + '\n')
    self.process.stdin.flush()
    assert select.select([self.process.stdout], [], [], 10)[0], 'peer response timeout'
    line = self.process.stdout.readline()
    assert line, ('peer exited', self.process.poll())
    return json.loads(line)

  def close(self):
    self.process.stdin.close()
    self.process.wait(timeout=10)
    self.log.close()
    assert self.process.returncode == 0


class Receiver:
  def __init__(self, scenario):
    self.scenario = scenario
    self.captures = []
    self.started = threading.Event()
    self.disconnected = threading.Event()
    self.active = 0
    self.maximum = 0
    self.lock = threading.Lock()
    owner = self

    class Handler(BaseHTTPRequestHandler):
      protocol_version = 'HTTP/1.1'

      def log_message(self, *args):
        pass

      def body(self):
        if self.headers.get('Transfer-Encoding') == 'chunked':
          chunks = []
          while True:
            size = int(self.rfile.readline().split(b';')[0], 16)
            if size == 0:
              assert self.rfile.readline() == b'\r\n'
              return b''.join(chunks)
            chunks.append(self.rfile.read(size))
            assert self.rfile.read(2) == b'\r\n'
        return self.rfile.read(int(self.headers.get('Content-Length', 0)))

      def reply(self, status, payload):
        body = b'' if status == 204 else json.dumps(payload).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        if status != 204:
          self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        try:
          self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
          owner.disconnected.set()

      def do_POST(self):
        body = self.body()
        owner.captures.append({'method': 'POST', 'path': unquote(self.path), 'body': json.loads(body), 'auth': self.headers.get('Authorization')})
        if self.path.endswith('/session'):
          self.reply(200, {'ok': True, 'token': 'synthetic-session'})
        elif self.path.endswith('/complete'):
          self.reply(503 if scenario == 'notify-fail' else 200, {'ok': False})
        else:
          self.reply(502 if scenario == 'notify-fail' else 204, {})

      def do_PUT(self):
        with owner.lock:
          owner.active += 1
          owner.maximum = max(owner.maximum, owner.active)
        try:
          body = self.body()
          path = unquote(self.path)
          attempt = sum(capture['method'] == 'PUT' and capture['path'] == path for capture in owner.captures)
          owner.captures.append(
            {'method': 'PUT', 'path': path, 'size': len(body), 'sha256': hashlib.sha256(body).hexdigest(), 'auth': self.headers.get('Authorization')}
          )
          owner.started.set()
          if scenario == 'stale':
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
              if select.select([self.connection], [], [], 0.05)[0] and self.connection.recv(1) == b'':
                owner.disconnected.set()
                return
            raise AssertionError('stale peer did not disconnect')
          time.sleep(0.6 if scenario == 'cancel' else 0.05)
          failed = (scenario == 'partial' and '--1/' in path) or (scenario == 'retry' and attempt == 0)
          self.reply(500 if failed else 200, {'ok': not failed, 'size': len(body), 'error': 'fixture rejection' if failed else ''})
        finally:
          with owner.lock:
            owner.active -= 1

    self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
    self.thread.start()
    self.base = f'http://127.0.0.1:{self.server.server_port}'

  def close(self):
    self.server.shutdown()
    self.server.server_close()
    self.thread.join()


def normalize(value, base):
  if isinstance(value, dict):
    return {
      key: normalize(child, base)
      for key, child in value.items()
      if key not in {'id', 'job_id', 'created_at', 'updated_at', 'bytes_per_second', 'uploadedAt', 'revision'}
    }
  if isinstance(value, list):
    return [normalize(child, base) for child in value]
  if isinstance(value, str):
    value = value.replace(base, 'http://fixture')
    if value.startswith('# Carrot Dashcam Upload'):
      value = '\n'.join(line for line in value.splitlines() if not line.startswith('- Time:'))
    return value
  return value


def run(command, output, scenario, concurrency):
  output.mkdir(parents=True)
  receiver = Receiver(scenario)
  if len(command) == 1:
    command = ['strace', '-f', '-qq', '-s', '200000', '-e', 'trace=write', '-o', str(output / 'writes.log'), *command]
  peer = Peer(command, output / 'process.log')
  try:
    with tempfile.TemporaryDirectory(prefix='dashcam-runtime-') as temp:
      root = Path(temp)
      segments = [f'00000001--1234567890--{index}' for index in range(4)]
      for index, segment in enumerate(segments):
        path = root / segment
        path.mkdir()
        (path / 'rlog.zst').write_bytes(bytes([index + 10]) * 1024)
        (path / 'qcamera.ts').write_bytes(bytes([index + 20]) * (2 * 1024 * 1024 + 17 if scenario == 'retry' else 4096))
        (path / 'qlog.zst').write_bytes(b'never upload this')
      if scenario in {'cancel', 'stale'}:
        segments = segments[:1]
      settings = {
        'root': str(root / 'wrong-settings-root') if scenario == 'root-selection' else str(root),
        'base_url': receiver.base,
        'token': '' if scenario == 'session' else 'synthetic-session',
        'metadata': {
          'carName': 'fixture car',
          'dongleId': 'test-device',
          'serial': 'test-serial',
          'branch': 'dev',
          'commit': '1234',
          'commitDate': '2026-09-30',
        },
        'webhook': receiver.base + '/webhook',
        'concurrency': concurrency,
      }
      start = peer.call(op='start', root=str(root), segments=segments, settings=settings)
      assert start['ok']
      job_id = start['job_id']
      duplicate = peer.call(op='start', root=str(root), segments=segments, settings=settings)
      assert not duplicate['ok'] and duplicate['error'] == 'upload already running'
      if scenario in {'cancel', 'stale'}:
        assert receiver.started.wait(5)
        if scenario == 'cancel':
          requested = peer.call(op='cancel', id=job_id)
          assert requested['cancel_requested'] and requested['phase'] == 'canceling'
        else:
          peer.call(op='expire', now=time.monotonic() + 1801)
          assert receiver.disconnected.wait(2), 'expired upload connection remained open'
      snapshots = []
      deadline = time.monotonic() + 10
      while True:
        snapshot = peer.call(op='snapshot', id=job_id)
        snapshots.append(snapshot)
        if snapshot.get('done'):
          break
        assert time.monotonic() < deadline, snapshot
        time.sleep(0.01)
      (output / 'snapshots.json').write_text(json.dumps(snapshots, indent=2))
      (output / 'captures.json').write_text(json.dumps(receiver.captures, indent=2))
      assert all(a['progress'] <= b['progress'] for a, b in zip(snapshots[:-1], snapshots[1:], strict=True))
      assert all(a['revision'] <= b['revision'] for a, b in zip(snapshots[:-1], snapshots[1:], strict=True))
      assert snapshot['revision'] > 0
      assert receiver.maximum <= max(1, min(6, concurrency))
      posts = [capture['path'] for capture in receiver.captures if capture['method'] == 'POST']
      if scenario in {'cancel', 'stale'}:
        assert not any(path.endswith('/complete') or path == '/webhook' for path in posts)
      else:
        assert sum(path.endswith('/complete') for path in posts) == 1
        assert posts[-1] == '/webhook'
      final = normalize(snapshot, receiver.base)
      if scenario in {'cancel', 'stale'}:
        for key in ['progress', 'revision', 'phase_current', 'bytes_current']:
          final.pop(key, None)
      final['log'] = '\n'.join(sorted(final['log'].splitlines()))
      uploads = sorted((capture['path'], capture['size'], capture['sha256'], capture['auth']) for capture in receiver.captures if capture['method'] == 'PUT')
      if scenario == 'retry':
        assert len(uploads) == len(segments) * 4
        assert snapshot['bytes_current'] == snapshot['bytes_total'] == len(segments) * (1024 + 2 * 1024 * 1024 + 17)
      result = {'final': final, 'uploads': uploads, 'maximum': receiver.maximum}
      (output / 'report.json').write_text(json.dumps(result, indent=2))
      return result
  finally:
    peer.close()
    receiver.close()


def verify_native_revisions(output):
  from check_dashcam_jobs import original

  buffers = {}
  packets = []
  pending = {}
  for line in (output / 'writes.log').read_text().splitlines():
    match = re.match(r'^(\d+)\s+write\(1, ("(?:\\.|[^"\\])*")', line)
    if match:
      pid = match[1]
      data = ast.literal_eval(match[2]).encode('latin-1')
      pending[pid] = data
    else:
      resumed = re.match(r'^(\d+)\s+<\.\.\. write resumed>', line)
      if not resumed:
        continue
      pid = resumed[1]
    complete = re.search(r'= (\d+)\s*$', line)
    if complete and pid in pending:
      buffers[pid] = buffers.get(pid, b'') + pending.pop(pid)[: int(complete[1])]
      while b'\n' in buffers[pid]:
        raw, buffers[pid] = buffers[pid].split(b'\n', 1)
        value = json.loads(raw)
        if isinstance(value, dict) and 'event' in value and 'clock' in value:
          packets.append(value)
  final = json.loads((output / 'snapshots.json').read_text())[-1]
  assert packets and packets[-1]['event'] == 'finish'
  segments = [item['segment'] for item in final['result']['results']]
  commands = [{'op': 'create', 'id': final['id'], 'segments': segments, 'clock': [final['created_at'], packets[0]['clock']['monotonic']]}]
  for packet in packets:
    event = packet['event']
    if event == 'context':
      continue
    fields = {key: value for key, value in packet.items() if key not in {'event', 'clock'}}
    commands.append({'op': event, 'id': final['id'], 'clock': [packet['clock']['wall'], packet['clock']['monotonic']], **fields})
  root = Path(__file__).resolve().parents[2]
  expected = original(root, commands)[-1]['jobs'][0]
  assert final == expected, ('actual worker events versus original job mutations', final, expected)
  (output / 'worker-packets.json').write_text(json.dumps(packets, indent=2))
  (output / 'revision-replay.json').write_text(json.dumps({'passed': True, 'packets': len(packets), 'revision': final['revision']}))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', action='store_true')
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path)
  parser.add_argument('--scenario')
  args = parser.parse_args()
  if args.source:
    asyncio.run(source_peer(*source_scope()))
    return
  args.output.mkdir(parents=True, exist_ok=True)
  source = [sys.executable, str(Path(__file__).resolve()), '--source']
  native = [str(args.binary.resolve())]
  reports = []
  for scenario, concurrency in [
    ('normal', 1),
    ('normal', 3),
    ('normal', 6),
    ('session', 3),
    ('partial', 3),
    ('notify-fail', 3),
    ('cancel', 1),
    ('stale', 1),
    ('retry', 3),
    ('root-selection', 3),
  ]:
    if args.scenario and scenario != args.scenario:
      continue
    name = f'{scenario}-{concurrency}'
    expected = run(source, args.output / (name + '-source'), scenario, concurrency)
    actual = run(native, args.output / (name + '-native'), scenario, concurrency)
    if scenario not in {'cancel', 'stale'}:
      verify_native_revisions(args.output / (name + '-native'))
    assert expected['final'] == actual['final'], (name, expected['final'], actual['final'])
    assert expected['uploads'] == actual['uploads'], (name, 'uploaded bodies differ')
    if scenario == 'normal':
      assert actual['maximum'] == min(concurrency, 4), (name, actual['maximum'])
    reports.append({'scenario': name, 'passed': True, 'source_concurrency': expected['maximum'], 'native_concurrency': actual['maximum']})
  (args.output / 'report.json').write_text(json.dumps(reports, indent=2) + '\n')
  print(json.dumps({'passed': len(reports)}))


if __name__ == '__main__':
  main()

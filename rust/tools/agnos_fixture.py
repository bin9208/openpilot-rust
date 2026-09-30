"""Deterministic local HTTP, regular-file partitions and harmless abctl fixture."""

from collections import Counter
import hashlib
import http.server
import json
import lzma
import random
import struct
import sys
import threading


def digest(data):
  return hashlib.sha256(data).hexdigest()


def caibx(chunks):
  output = struct.pack('<6Q', 48, 0x96824D9C7B129FF9, 1234, 1, 1024, 1024 * 1024)
  output += struct.pack('<2Q', 16 + 40 * (len(chunks) + 1), 0xE75B9E112F17417D)
  offset = 0
  for data in chunks:
    offset += len(data)
    output += struct.pack('<Q', offset) + hashlib.new('sha512_256', data).digest()
  return output + b'\0' * 40


def sparse_image():
  block = 16
  output = struct.pack('<I4H4I', 0xED26FF3A, 1, 0, 28, 12, block, 5, 4, 0)
  output += struct.pack('<H2xII', 0xCAC1, 1, 12 + block) + b'A' * block
  output += struct.pack('<H2xII', 0xCAC2, 2, 16) + b'FILL'
  output += struct.pack('<H2xII', 0xCAC3, 1, 12)
  output += struct.pack('<H2xII', 0xCAC1, 1, 12 + block) + b'Z' * block
  return output, b'A' * block + b'FILL' * 8 + b'Z' * block


class Server:
  def __init__(self):
    self.files, self.requests, self.counts = {}, [], Counter()
    self.mode = ''
    owner = self

    class Handler(http.server.BaseHTTPRequestHandler):
      protocol_version = 'HTTP/1.1'

      def log_message(self, *args):
        pass

      def do_GET(self):
        owner.counts[self.path] += 1
        owner.requests.append({'path': self.path, 'range': self.headers.get('Range')})
        if self.path.endswith('.cacnk'):
          owner.requests[-1]['cookie'] = self.headers.get('Cookie')
        if self.path not in owner.files:
          body, status = b'missing', 404
        elif owner.mode == 'retry' and owner.counts[self.path] <= 5:
          body, status = b'busy', 503
        elif owner.mode == 'fatal':
          body, status = b'forbidden', 404
        else:
          body, status = owner.files[self.path], 200
        if status == 200 and self.headers.get('Range') and 'ignore' not in self.path:
          offset = int(self.headers['Range'].removeprefix('bytes=').removesuffix('-'))
          body, status = body[offset:], 206
        truncate = 'truncated' in self.path or (owner.mode == 'casync_retry' and self.path.endswith('.cacnk') and owner.counts[self.path] <= 2)
        self.send_response(status)
        if self.path.endswith('.cacnk'):
          self.send_header('Set-Cookie', 'chunk=fixture; Path=/')
        self.send_header('Content-Length', str(len(body) + 23 if truncate else len(body)))
        if truncate:
          self.send_header('Connection', 'close')
        self.end_headers()
        self.wfile.write(body)
        self.wfile.flush()
        if truncate:
          self.close_connection = True

    self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.address = f'http://127.0.0.1:{self.server.server_port}'
    self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
    self.thread.start()

  def reset(self):
    self.requests.clear()
    self.counts.clear()

  def close(self):
    self.server.shutdown()
    self.server.server_close()
    self.thread.join(timeout=2)


class Fixture:
  def __init__(self, root, server, launcher, scenario):
    self.root = root
    self.parts, self.cache = root / 'partitions', root / 'cache'
    self.parts.mkdir()
    self.cache.mkdir()
    self.config = {
      'paths': {
        'partitions': str(self.parts),
        'cache': str(self.cache),
        'confirmation': str(root / 'confirmation'),
        'lock': str(root / 'lock'),
        'caibx_url': server.address + '/seed/',
      },
      'abctl': str(root / 'abctl'),
      'launcher': str(launcher),
    }
    script = root / 'abctl'
    script.write_text(f'''#!{sys.executable}
import json, sys
from pathlib import Path
root = Path({str(root)!r})
with (root / 'calls.jsonl').open('a') as out: out.write(json.dumps(sys.argv[1:]) + '\\n')
if sys.argv[1] == '--boot_slot': print('_a')
elif sys.argv[1] == '--set_active':
  path = root / 'swaps'
  count = int(path.read_text()) + 1 if path.exists() else 1
  path.write_text(str(count))
  if count == 1: print('No such file or directory; lun as boot lun')
  elif count == 2: print('pending', flush=True); print('retry', file=sys.stderr); sys.exit(1)
  else: print('marked lun as boot lun', file=sys.stderr)
''')
    script.chmod(0o755)
    self.manifest = root / 'manifest.json'
    self.operations = []
    server.mode = scenario if scenario in ('retry', 'fatal') else ''
    raw = random.Random(42).randbytes(2 * 1024 * 1024 + 37)
    small = b'fixture-image' * 500

    def partition(name, data=small, path=None, full=True, sparse=False, raw_data=None, cached=True):
      path = path or '/' + name + '.xz'
      packed = lzma.compress(data)
      server.files[path] = packed
      (self.parts / (name + '_b')).write_bytes(b'old partition')
      item = {
        'name': name,
        'size': len(raw_data if raw_data is not None else data),
        'hash_raw': digest(raw_data if raw_data is not None else data),
        'hash': digest(data),
        'full_check': full,
        'sparse': sparse,
        'url': server.address + path,
      }
      if cached:
        item.update(compressed_hash=digest(packed), compressed_size=len(packed))
      return item, packed

    if scenario == 'helpers':
      manifest = [
        {'url': 'https://EXAMPLE.test/a'},
        {'url': 'https://EXAMPLE.test/b'},
        {'url': 'https://example.test/c'},
        {'url': 'relative/path'},
        {'url': '/other'},
        {'url': 3},
        {'url': 'ftp://host/path'},
      ]
      self.operations = [{'op': 'helpers'}, {'op': 'target'}]
    elif scenario == 'download':
      boot, packed = partition('boot', raw)
      (self.cache / f'boot-{boot["compressed_hash"]}.img.xz.part').write_bytes(packed[:12345])
      sparse, raw_sparse = sparse_image()
      system, packed = partition('system', sparse, path='/ignore.xz', sparse=True, raw_data=raw_sparse)
      (self.cache / f'system-{system["compressed_hash"]}.img.xz.part').write_bytes(packed[:20])
      manifest = [boot, system]
      huge = root / 'high-compression.xz'
      huge.write_bytes(lzma.compress(b'Z' * (4 * 1024 * 1024 + 13)))
      self.operations = [
        {'op': 'cache', 'partition': boot},
        {'op': 'flash', 'standalone': True, 'retry_network': False},
        {'op': 'verify', 'partition': system, 'force': True},
        {'op': 'decompress', 'path': str(huge), 'reads': [1, 7, 1024, 3 * 1024 * 1024, 2 * 1024 * 1024, 1]},
      ]
    elif scenario == 'marker':
      item, _ = partition('boot', full=False)
      (self.parts / 'boot_b').write_bytes(small + item['hash_raw'].encode())
      manifest = [item]
      self.operations = [
        {'op': 'verify', 'partition': item, 'force': True},
        {'op': 'clear', 'partition': item},
        {'op': 'flash', 'standalone': True, 'retry_network': False},
        {'op': 'verify', 'partition': item, 'force': False},
        {'op': 'verify', 'partition': item, 'force': True},
        {'op': 'swap'},
      ]
    elif scenario in ('retry', 'fatal', 'cli'):
      item, _ = partition('boot', cached=False)
      if scenario == 'cli':
        item.update(casync_caibx='must-not-be-used-by-standalone', casync_store='unused')
      manifest = [item]
      self.operations = [{'op': 'flash', 'standalone': True, 'retry_network': True}]
    elif scenario == 'corruption':
      cache_bad, _ = partition('cachebad')
      cache_bad['compressed_hash'] = '0' * 64
      raw_bad, _ = partition('rawbad')
      raw_bad['hash_raw'] = '0' * 64
      truncated, _ = partition('short', path='/truncated.xz')
      manifest = [raw_bad]
      invalid = raw_bad | {'size': 'bad'}
      self.operations = [
        {'op': 'cache', 'partition': cache_bad},
        {'op': 'compressed', 'partition': raw_bad},
        {'op': 'cache', 'partition': truncated},
        {'op': 'verify', 'partition': invalid, 'force': True},
      ]
    elif scenario == 'casync':
      chunks = [b'A' * 1024, b'B' * 1024, b'C' * 1024, b'A' * 1024]
      desired = b''.join(chunks)
      seed = chunks[0] + b'old' * 1024
      (self.parts / 'system_a').write_bytes(seed)
      (self.parts / 'system_b').write_bytes(b'\0' * 1024 + chunks[1] + b'\0' * 2048)
      server.files['/target.caibx'] = caibx(chunks)
      server.files['/seed/system-' + digest(seed) + '.caibx'] = caibx([chunks[0]])
      sha = hashlib.new('sha512_256', chunks[2]).hexdigest()
      server.files[f'/store/{sha[:4]}/{sha}.cacnk'] = lzma.compress(chunks[2])
      server.mode = 'casync_retry'
      manifest = [
        {
          'name': 'system',
          'size': len(desired),
          'hash_raw': digest(desired),
          'hash': digest(desired),
          'full_check': True,
          'sparse': False,
          'url': 'unused',
          'casync_caibx': server.address + '/target.caibx',
          'casync_store': server.address + '/store',
        }
      ]
      self.operations = [{'op': 'flash', 'standalone': False, 'retry_network': False}]
    else:
      raise ValueError(scenario)
    self.manifest.write_text(json.dumps(manifest))
    self.request = {'config': self.config, 'manifest': str(self.manifest), 'operations': self.operations}

  def snapshot(self):
    files = {}
    for prefix, directory in [('partitions', self.parts), ('cache', self.cache)]:
      for path in sorted(directory.iterdir()):
        data = path.read_bytes()
        files[prefix + '/' + path.name] = {'length': len(data), 'sha256': digest(data)}
    calls = [json.loads(line) for line in (self.root / 'calls.jsonl').read_text().splitlines()] if (self.root / 'calls.jsonl').exists() else []
    return {'files': files, 'calls': calls}

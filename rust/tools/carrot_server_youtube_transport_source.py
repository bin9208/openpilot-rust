# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Original ctypes transport against caller-owned loopback recipients."""

from __future__ import annotations

import ipaddress
import json
import os
from pathlib import Path
import sys
import threading
import time
from urllib.parse import urlsplit

from carrot_server_dashcam_upload import save


def main() -> None:
  value = json.loads(sys.stdin.readline())
  endpoint = urlsplit(value['url'])
  assert ipaddress.ip_address(endpoint.hostname).is_loopback
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(Path(value['owned_root']))
  from openpilot.selfdrive.carrot.server.services.youtube_live_transport import LibrtmpClient, RtmpSink

  if value.get('repeat_urls'):
    rows = []
    for url in value['repeat_urls']:
      client = LibrtmpClient(url)
      client.connect()
      sink = RtmpSink(client)
      sink.write(Path(value['input']).read_bytes())
      sink.flush()
      written = client.bytes_written
      client.close()
      del sink, client
      rows.append({'bytes_written': written, 'library_mapped_after_client_drop': '/librtmp.so.1' in Path('/proc/self/maps').read_text()})
    print(json.dumps(rows))
    return
  client = LibrtmpClient(value['url'])
  sink = RtmpSink(client)
  started = time.monotonic()
  error = None
  monitor = None
  try:
    client.connect()
    if value.get('pause_after_connect'):
      print(json.dumps({'ready': True}), flush=True)
      sys.stdin.readline()
    if value.get('input'):
      data = Path(value['input']).read_bytes()
      if value.get('monitor_blocked'):
        completed = []

        def write() -> None:
          began = time.monotonic()
          failure = None
          try:
            sink.write(data)
            sink.flush()
          except (RuntimeError, OSError, ValueError) as caught:
            failure = str(caught)
          completed.append((time.monotonic() - began, failure))

        worker = threading.Thread(target=write, name='owned-blocked-rtmp-writer')
        worker.start()
        time.sleep(2)
        health = client.try_is_connected()
        progress = client.bytes_written
        if value.get('force_wake'):
          assert client._tunnel
          client._tunnel.close()
        worker.join(timeout=12)
        assert not worker.is_alive()
        seconds, error = completed[0]
        monitor = {
          'health_during_write': health,
          'progress_during_write': progress,
          'force_wake': bool(value.get('force_wake')),
          'write_seconds': seconds,
          'worker_joined': True,
        }
      elif value.get('omit_carry_fixture'):
        client.write(data)
      else:
        for offset in range(0, len(data), value['chunk']):
          sink.write(data[offset : offset + value['chunk']])
        sink.flush()
  except (RuntimeError, OSError, ValueError) as failure:
    error = str(failure)
  result = {
    'ok': error is None,
    'error': error,
    'bytes_written': client.bytes_written,
    'connected': client.try_is_connected(),
    'bytes_accepted': sink.bytes_accepted,
    'pending_bytes': sink.pending_bytes,
    'drain_calls': sink.drain_calls,
    'partial_writes': sink.partial_writes,
    'seconds': time.monotonic() - started,
  }
  if monitor:
    result['monitor'] = monitor
  client.close()
  save(Path(value['receipt']), result)
  print(json.dumps(result))


if __name__ == '__main__':
  main()

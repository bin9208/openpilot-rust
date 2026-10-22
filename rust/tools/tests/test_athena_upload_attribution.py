"""Hosted-only boundary tests for the actual upload-edges fixture handler."""
import hashlib
import http.client
import importlib.util
from pathlib import Path
import sys
import socket
import threading
import time
import unittest  # noqa: TID251 - stdlib-only owned HTTP boundary gate
from http.server import ThreadingHTTPServer

TOOLS = Path(__file__).resolve().parents[1]


def fixture_module():
  sys.path.insert(0, str(TOOLS))
  try:
    spec = importlib.util.spec_from_file_location('owned_upload_edges', TOOLS / 'check_athena_upload_edges.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
  finally:
    sys.path.pop(0)


class UploadAttribution(unittest.TestCase):
  def setUp(self):
    self.module = fixture_module()

  def serve(self, handler):
    class QuietServer(ThreadingHTTPServer):
      def handle_error(self, *_):
        # Intentional response-write failure is asserted via the trace receipt.
        pass
    server = QuietServer(('127.0.0.1', 0), handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    self.addCleanup(server.server_close)
    self.addCleanup(thread.join, 5)
    self.addCleanup(server.shutdown)
    return http.client.HTTPConnection('127.0.0.1', server.server_port, timeout=5)

  def test_real_response_flush_is_separate_from_unchanged_comparison_row(self):
    rows = []
    trace = self.module.RequestTrace()
    peer = self.serve(self.module.upload_handler(rows, b'owned', trace))
    self.addCleanup(peer.close)
    peer.request('PUT', '/native/redirect303', body=b'owned')
    response = peer.getresponse()
    self.assertEqual(response.status, 303)
    self.assertEqual(response.getheader('Location'), '/native/redirect303/final')
    self.assertEqual(response.read(), b'')
    # Synchronize with handler completion without weakening a response receipt.
    events = trace.snapshot()['events']
    deadline = time.monotonic() + 5
    while not any(event['event'] == 'response_flushed' for event in events) and time.monotonic() < deadline:
      time.sleep(0.001)
      events = trace.snapshot()['events']
    self.assertEqual(rows, [{'path': '/native/redirect303', 'method': 'PUT', 'status': 303,
                            'length': 5, 'sha256': hashlib.sha256(b'owned').hexdigest(), 'decoded_sha256': None}])
    self.assertEqual([event['event'] for event in events], ['request_accepted', 'body_complete', 'response_flushed'])
    self.assertEqual({event['request'] for event in events}, {1})
    self.assertEqual(events[0]['lane'], 'native')
    self.assertEqual(events[0]['endpoint'], 'redirect303')
    self.assertEqual(events[-1]['status'], 303)
    self.assertTrue(all(event['mono_ns'] > 0 and event['wall_ns'] > 0 for event in events))
    self.assertEqual([event['mono_ns'] for event in events], sorted(event['mono_ns'] for event in events))

  def test_failed_header_write_never_claims_response_flushed(self):
    rows = []
    trace = self.module.RequestTrace()
    class BrokenHeaders(self.module.upload_handler(rows, b'owned', trace)):
      def end_headers(self):
        raise BrokenPipeError('SENSITIVE_ERROR_TEXT')
    peer = self.serve(BrokenHeaders)
    self.addCleanup(peer.close)
    peer.request('PUT', '/native/redirect303', body=b'owned')
    with self.assertRaises(http.client.RemoteDisconnected):
      peer.getresponse()
    events = trace.snapshot()['events']
    self.assertEqual(len(rows), 1)
    self.assertEqual(rows[0]['status'], 303)  # Intended comparison status only.
    self.assertEqual([event['event'] for event in events], ['request_accepted', 'body_complete', 'error'])
    self.assertEqual(events[-1]['phase'], 'response_write')
    self.assertEqual(events[-1]['category'], 'broken_pipe')
    self.assertNotIn('SENSITIVE_', str(events))
    self.assertNotIn('status', events[-1])

  def test_short_body_does_not_claim_completion_or_change_old_response_behavior(self):
    rows = []
    trace = self.module.RequestTrace()
    peer = self.serve(self.module.upload_handler(rows, b'owned', trace))
    self.addCleanup(peer.close)
    with socket.create_connection((peer.host, peer.port), timeout=5) as connection:
      connection.sendall(b'PUT /native/200 HTTP/1.0\r\nContent-Length: 8\r\n\r\nx')
      connection.shutdown(socket.SHUT_WR)
      response = b''
      while chunk := connection.recv(1024):
        response += chunk
    self.assertIn(b'200 OK', response)
    self.assertEqual(rows[0]['length'], 1)
    events = trace.snapshot()['events']
    self.assertEqual([event['event'] for event in events], ['request_accepted', 'error', 'response_flushed'])
    self.assertEqual(events[1]['phase'], 'body_read')
    self.assertEqual(events[1]['category'], 'unexpected_eof')
    self.assertEqual(events[1]['declared_bytes'], 8)
    self.assertEqual(events[1]['received_bytes'], 1)

  def test_trace_is_bounded_and_overflow_does_not_change_rows(self):
    trace = self.module.RequestTrace(limit=4)
    for _ in range(3):
      request = trace.begin('PUT', '/native/redirect303', 12345)
      trace.record(request, 'body_complete', received_bytes=5, declared_bytes=5)
    snapshot = trace.snapshot()
    self.assertEqual(len(snapshot['events']), 4)
    self.assertEqual(snapshot['dropped'], 2)
    self.assertEqual([event['request'] for event in snapshot['events']], [1, 1, 2, 2])


if __name__ == '__main__':
  unittest.main()

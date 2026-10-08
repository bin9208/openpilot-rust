"""Owned loopback SSH-key endpoint; pending responses remain gated until cleanup."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Event, Lock, Thread
from urllib.parse import unquote


class Server:
  def __init__(self):
    self.release = Event()
    self.lock = Lock()
    self.requests = []
    owner = self

    class Handler(BaseHTTPRequestHandler):
      def do_GET(self):
        path = unquote(self.path)
        with owner.lock:
          owner.requests.append({'path': path, 'user_agent': self.headers.get('User-Agent')})
        if path == '/pending-user.keys':
          owner.release.wait()
        code = 404 if path == '/missing-user.keys' else 200
        content = b' \n' if path == '/empty-user.keys' else b' \nssh-ed25519 fixture-key\n '
        self.send_response(code)
        self.send_header('Content-Type', 'text/plain; charset=utf-8')
        self.send_header('Content-Length', str(len(content)))
        self.end_headers()
        try:
          self.wfile.write(content)
        except (BrokenPipeError, ConnectionResetError):
          with owner.lock:
            owner.requests.append({'client_closed_pending_response': path})

      def log_message(self, *_args):
        return

    self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.host = f'http://127.0.0.1:{self.server.server_port}'
    self.thread = Thread(target=self.server.serve_forever, daemon=True)

  def __enter__(self):
    self.thread.start()
    return self

  def __exit__(self, *_args):
    self.release.set()
    self.server.shutdown()
    self.server.server_close()
    self.thread.join(timeout=5)
    assert not self.thread.is_alive()

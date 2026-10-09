# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Imported by carrot_server_dashcam_health.py; source dependencies are supplied by the caller.
from __future__ import annotations

from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading


@dataclass(frozen=True, slots=True)
class Case:
    name: str
    health_status: int = 200
    token: str = ''
    session: str = 'normal'
    environment_url: bool = False
    invalid_url: bool = False
    method: str = 'POST'
    expected_status: int = 200
    held: str = ''
    unrelated_non_utf8: bool = False


class Receiver:
    def __init__(self, case: Case):
        self.captures = []
        self.started = threading.Event()
        self.release = threading.Event()
        self.disconnected = threading.Event()
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args) -> None:
                return

            def hold(self, phase: str) -> None:
                if case.held == phase:
                    owner.started.set()
                    assert owner.release.wait(10)

            def observe_close(self, phase: str) -> None:
                if case.held == phase:
                    self.connection.settimeout(3)
                    if self.connection.recv(1) == b'':
                        owner.disconnected.set()

            def do_GET(self) -> None:
                owner.captures.append({'method': 'GET', 'path': self.path, 'authorization': self.headers.get('Authorization', '')})
                self.hold('health')
                body = b'owned receiver unhealthy' if case.health_status != 200 else b'healthy'
                self.send_response(case.health_status)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                self.observe_close('health')

            def do_POST(self) -> None:
                payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                owner.captures.append({'method': 'POST', 'path': self.path, 'authorization': self.headers.get('Authorization', ''), 'payload': payload})
                self.hold('session')
                body = {'ok': True, 'token': 'owned-session'}
                status = 200
                if case.session == 'error':
                    body = {'ok': False, 'error': 'owned session rejected'}
                    status = 403
                if case.session == 'missing-token':
                    body = {'ok': True}
                encoded = b'owned non-json' if case.session == 'non-json' else json.dumps(body).encode()
                self.send_response(status)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(encoded)))
                self.end_headers()
                self.wfile.write(encoded)
                self.observe_close('session')

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        self.base = f'http://127.0.0.1:{self.server.server_port}'

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        assert not self.thread.is_alive()

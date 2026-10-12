# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Imported by the sync probe; uses the existing owned upload receiver protocol.
from __future__ import annotations

import hashlib
import select
import socket
import threading
from urllib.parse import unquote

from check_dashcam_runtime import Receiver


class Reader:
    def __init__(self, reader, eof: threading.Event):
        self.reader = reader
        self.eof = eof

    def readline(self, *args):
        result = self.reader.readline(*args)
        if not result:
            self.eof.set()
        return result

    def read(self, *args):
        return self.reader.read(*args)

    def close(self):
        self.reader.close()


class HeldReceiver(Receiver):
    def __init__(self, held: bool = True):
        self.release = threading.Event()
        if not held:
            self.release.set()
        self.eof_before_release = threading.Event()
        super().__init__('sync-owned')
        owner = self
        handler = self.server.RequestHandlerClass
        setup = handler.setup

        def observed_setup(peer):
            setup(peer)
            peer.rfile = Reader(peer.rfile, owner.disconnected)

        def put(peer):
            body = peer.body()
            owner.captures.append({'method': 'PUT', 'path': unquote(peer.path), 'size': len(body), 'sha256': hashlib.sha256(body).hexdigest(), 'auth': peer.headers.get('Authorization')})
            owner.started.set()
            while not owner.release.wait(.01):
                if select.select([peer.connection], [], [], 0)[0] and peer.connection.recv(1, socket.MSG_PEEK) == b'':
                    owner.eof_before_release.set(); owner.disconnected.set()
                    return
            peer.reply(200, {'ok': True, 'size': len(body), 'error': ''})

        handler.setup = observed_setup
        handler.do_PUT = put

    def close(self):
        self.release.set()
        super().close()
        assert not self.thread.is_alive()

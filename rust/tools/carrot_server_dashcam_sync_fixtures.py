# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Shared owned fixture plumbing for the sync HTTP oracle.
from __future__ import annotations

import base64
from dataclasses import dataclass
from enum import StrEnum
import json
import os
from pathlib import Path
import sys
from typing import TypedDict, assert_never

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_probe import Json, Peer, environment, startup
from carrot_server_dashcam_upload import save
from check_dashcam_runtime import Receiver, normalize


class Kind(StrEnum):
    SOURCE = 'source'
    NATIVE = 'native'


@dataclass(frozen=True, slots=True)
class Config:
    binary: Path
    worker: Path
    repository: Path
    output: Path
    composed: bool = False
    token: str = 'owned-static-token'
    git_shim: Path | None = None


@dataclass(frozen=True, slots=True)
class Call:
    path: str = '/api/dashcam/upload'
    body: Json = None
    method: str = 'POST'


class Response(TypedDict):
    status: int
    payload: Json
    headers: dict[str, str]
    body_base64: str


class Fixture:
    def __init__(self, kind: Kind, config: Config):
        self.kind = kind; self.config = config; self.peer = Peer(config.output/kind)
        self.peer.output.mkdir()

    async def start(self, root: Path, receiver: Receiver) -> None:
        env = environment(self.peer.output, receiver.base, self.config.repository)
        env['CARROT_WEB_UPLOAD_TOKEN'] = self.config.token
        if self.config.git_shim:
            env['PATH'] = str(self.config.git_shim)+os.pathsep+env['PATH']
        data = {'root': str(root), 'state': env['CARROT_DATA_DIR'], 'output': str(self.peer.output), 'repository': str(self.config.repository)}
        match self.kind:
            case Kind.SOURCE:
                command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_dashcam_sync_http_source.py'))]
            case Kind.NATIVE:
                command = [str(self.config.binary)]
                data.update(worker=str(self.config.worker), settings=None, composed=self.config.composed)
            case unreachable:
                assert_never(unreachable)
        await startup(self.peer, self.peer.start(command, data, env, True))

    async def connect(self, call: Call):
        stream = await anyio.connect_tcp('127.0.0.1', self.peer.ready['port'])
        body = b'' if call.body is None else json.dumps(call.body).encode()
        await stream.send(f'{call.method} {call.path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {len(body)}\r\n\r\n'.encode()+body)
        return stream

    async def fetch(self, call: Call) -> Response:
        stream = await self.connect(call)
        try:
            with anyio.fail_after(12):
                response = await read_response(BufferedByteReceiveStream(stream), call.method)
            encoded = base64.b64decode(response['body_base64'])
            response['payload'] = json.loads(encoded) if encoded.startswith((b'{', b'[')) else encoded.decode()
            return response
        finally:
            await stream.aclose()

    async def force(self, action: str = 'force') -> None:
        await self.peer.process.stdin.send((action+'\n').encode())
        await self.peer.process.stdin.aclose()
        self.peer.stopped = True


def compare(source: Response, native: Response, receivers: tuple[Receiver, Receiver]) -> None:
    assert source['status'] == native['status']
    assert normalize(source['payload'], receivers[0].base) == normalize(native['payload'], receivers[1].base)


def children(pid: int) -> dict[int, str]:
    result = {}
    for path in Path(f'/proc/{pid}/task').glob('*/children'):
        for value in path.read_text().split():
            child = int(value); stat = Path(f'/proc/{child}/stat')
            if stat.exists(): result[child] = stat.read_text().split(') ', 1)[1].split()[19]
    return result


def exited(identities: dict[int, str]) -> bool:
    for pid, start in identities.items():
        path = Path(f'/proc/{pid}/stat')
        if path.exists() and path.read_text().split(') ', 1)[1].split()[19] == start:
            return False
    return True


async def close(fixtures: list[Fixture], receivers: list[Receiver]) -> None:
    errors = []
    for receiver in receivers:
        release = getattr(receiver, 'release', None)
        if release: release.set()
    for fixture in fixtures:
        try: await fixture.peer.close()
        except (AssertionError, OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
            errors.append(f'{fixture.kind}: {type(error).__name__}: {error}')
    for receiver in receivers:
        try: await anyio.to_thread.run_sync(receiver.close)
        except (AssertionError, OSError) as error: errors.append(f'receiver: {error}')
    if fixtures: save(fixtures[0].config.output/'cleanup.json', {'errors': errors})
    assert not errors

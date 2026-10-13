#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_dashcam_report_http.py --binary PATH --captures PATH --output NEW_DIR
from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
from typing import TypedDict

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_probe import Json, Peer, startup


class Response(TypedDict):
    status: int
    headers: dict[str,str]
    body_base64: str


@dataclass(frozen=True, slots=True)
class Call:
    path: str
    method: str = 'GET'


def save(path: Path, value: Json) -> None:
    path.write_text(json.dumps(value,allow_nan=True,indent=2)+'\n')


class Fixture:
    def __init__(self, output: Path):
        self.output = output; output.mkdir(); self.peer = Peer(output)

    async def start(self, root: Path, binary: Path | None, composed: bool) -> None:
        env = os.environ.copy(); env['TZ'] = 'Asia/Seoul'
        env['CARROT_DATA_DIR'] = str(self.output/'data')
        env['PARAMS_ROOT'] = str(self.output/'params'); env['OPENPILOT_PREFIX'] = 'owned-report'
        command = [str(binary)] if binary else [sys.executable,'-P',str(Path(__file__).with_name('carrot_server_dashcam_report_http_source.py'))]
        data = {'root': str(root), 'state': str(self.output/'state'), 'output': str(self.output), 'composed': composed}
        await startup(self.peer,self.peer.start(command,data,env,True))

    async def fetch(self, call: Call) -> Response:
        async with await anyio.connect_tcp('127.0.0.1',self.peer.ready['port']) as stream:
            await stream.send(f'{call.method} {call.path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'.encode())
            with anyio.fail_after(12):
                return await read_response(BufferedByteReceiveStream(stream),call.method)


def comparable(response: Response, root: Path) -> tuple[int, str, dict[str, str]]:
    raw = base64.b64decode(response['body_base64'])
    original = raw.decode(); body = original.replace(str(root),'<ROOT>')
    headers = {key:value for key,value in response['headers'].items() if key in ('content-type','content-length','allow')}
    if body != original and 'content-length' in headers:
        headers['content-length'] = str(int(headers['content-length']) - len(raw) + len(body.encode()))
    return response['status'], body, headers


def calls(name: str, route: str) -> list[Call]:
    path = '/api/dashcam/report/'+route
    if name == 'preference':
        return [Call(path),Call(path,'HEAD'),Call(path+'?source=qlog'),Call(path+'?source=qlog&source=rlog'),
            Call(path+'--0'),Call('/api/dashcam/report/missing'),Call(path,'POST')]
    return [Call(path)]


async def scenario(binary: Path, captures: Path, output: Path, name: str, reference: Path | None) -> int:
    route = '2026-01-02--03-04-05'; output.mkdir()
    template = captures/('raw' if name == 'permission' else name)
    source_root = output/'source-root'; native_root = output/'native-root'
    for root in [source_root,native_root]:
        root.mkdir()
        for directory in template.iterdir():
            if directory.is_dir(): shutil.copytree(directory,root/directory.name)
    peers = [Fixture(output/'source'),Fixture(output/'native')] if reference is None else [Fixture(output/'native')]
    rows = []; responses = []
    try:
        if reference is None:
            await peers[0].start(source_root,None,False)
        native = peers[-1]; await native.start(native_root,binary,reference is not None)
        if name == 'permission':
            source_root.chmod(0); native_root.chmod(0)
        selected = calls(name,route) if reference is None else [Call('/api/params_bulk?names=OwnedReportProbe'),Call('/api/dashcam/report/'+route),Call('/api/params_bulk?names=OwnedReportProbe')]
        for index, call in enumerate(selected):
            actual = await native.fetch(call); responses.append(actual)
            if reference is None:
                expected = await peers[0].fetch(call)
                assert comparable(expected,source_root) == comparable(actual,native_root)
                rows.append({'call': {'path':call.path,'method':call.method},'source':expected,'native':actual})
            elif index == 1:
                saved = json.loads((reference/name/'responses.json').read_text())['pairs'][0]['source']
                saved_root = reference/name/'source-root'
                expected = comparable(saved,saved_root)
                received = comparable(actual,native_root)
                save(output/'reference-comparison.json',{'expected':expected,'native':received,'equal':expected==received})
                assert expected == received
            else: assert actual['status'] == 200
        status = 500 if name == 'permission' else 200
        assert responses[1 if reference is not None else 0]['status'] == status
        if name == 'nan-speed': assert b'NaN' in base64.b64decode(responses[0]['body_base64'])
    finally:
        failed = sys.exc_info()[0] is not None
        source_root.chmod(0o700); native_root.chmod(0o700)
        save(output/'responses.json', {'pairs':rows,'native_responses':responses})
        errors = []
        for fixture in peers:
            try: await fixture.peer.close()
            except (AssertionError,OSError,TimeoutError,anyio.BrokenResourceError,anyio.ClosedResourceError) as error:
                errors.append(f'{type(error).__name__}: {error}')
        save(output/'cleanup.json',{'errors':errors})
        if not failed: assert not errors
    return len(rows) if reference is None else len(responses)


async def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--captures',type=Path,required=True); parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--app-reference',type=Path)
    args = parser.parse_args(); output = args.output.resolve(); output.mkdir(parents=True)
    binary = args.binary.resolve(); reference = args.app_reference.resolve() if args.app_reference else None
    save(output/'invocation.json',{'command':[sys.executable,'-P',*sys.argv], 'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(), 'scope':'new reportHTTP or Appseam only;51standalone comparisons reused', 'normalization':'ownedRoot path in errorJSON and its ContentLength delta only; rawlength consumed exactly by shared read_response'})
    count = 0
    for name in (('preference','nan-speed','permission') if reference is None else ('preference','permission')):
        count += await scenario(binary,args.captures.resolve(),output/name,name,reference)
    save(output/'result.json',{'paired_http':count if reference is None else 0,'native_app_responses':count if reference is not None else 0,'differences':0})
    print(json.dumps({'responses':count,'differences':0}))


if __name__ == '__main__':
    anyio.run(main)

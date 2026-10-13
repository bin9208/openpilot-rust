# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Called by the sync oracle to observe independent calls and existing jobs.
from __future__ import annotations

from dataclasses import replace
import hashlib
from pathlib import Path

import anyio
from carrot_server_dashcam_sync_fixtures import Call, Config, Fixture, Kind, children, close, compare, exited
from carrot_server_dashcam_sync_peer import HeldReceiver
from carrot_server_dashcam_sync_probe import files
from carrot_server_dashcam_upload import save


async def held_count(receiver: HeldReceiver, count: int) -> None:
    with anyio.fail_after(4):
        while sum(row['method'] == 'PUT' and row['path'].endswith('qcamera.ts') for row in receiver.captures) < count:
            await anyio.sleep(.01)


async def two_calls(config: Config) -> None:
    output = config.output/'two-sync'; output.mkdir()
    current = replace(config, output=output); root, segment = files(output)
    receivers = [HeldReceiver() for _ in Kind]; fixtures = [Fixture(kind, current) for kind in Kind]
    records = []
    try:
        for fixture, receiver in zip(fixtures, receivers, strict=True): await fixture.start(root, receiver)
        for fixture, receiver in zip(fixtures, receivers, strict=True):
            replies = [None, None]
            async def run(index: int) -> None:
                replies[index] = await fixture.fetch(Call(body={'segment': segment}))
            async with anyio.create_task_group() as tasks:
                tasks.start_soon(run, 0); tasks.start_soon(run, 1)
                await held_count(receiver, 2)
                identities = children(fixture.peer.process.pid) if fixture.kind == Kind.NATIVE else {}
                assert not receiver.eof_before_release.is_set() and replies == [None, None]
                if fixture.kind == Kind.NATIVE: assert len(identities) == 2
                receiver.release.set()
            assert exited(identities)
            records.append({'kind': fixture.kind, 'responses': replies, 'owned_workers_while_held': identities,
                            'owned_workers_reaped': exited(identities), 'recipient_requests': len(receiver.captures)})
            assert len(receiver.captures) == 8
        for index in range(2): compare(records[0]['responses'][index], records[1]['responses'][index], tuple(receivers))
    finally:
        save(output/'result.json', records)
        for kind, receiver in zip(Kind, receivers, strict=True): save(output/f'{kind}-receiver.json', receiver.captures)
        await close(fixtures, receivers)


def selected_hold() -> HeldReceiver:
    receiver = HeldReceiver()
    handler = receiver.server.RequestHandlerClass
    original = handler.do_PUT
    def put(peer):
        if '--0/' in peer.path:
            original(peer)
        else:
            body = peer.body()
            receiver.captures.append({'method': 'PUT', 'path': peer.path, 'size': len(body),
                'sha256': hashlib.sha256(body).hexdigest(), 'auth': peer.headers.get('Authorization')})
            peer.reply(200, {'ok': True, 'size': len(body), 'error': ''})
    handler.do_PUT = put
    return receiver


async def alongside_job(config: Config) -> None:
    output = config.output/'alongside-job'; output.mkdir()
    current = replace(config, output=output); root, segment = files(output)
    other = '00000001--1234567890--1'; (root/other).mkdir()
    (root/other/'qcamera.ts').write_bytes(b'Q'*4096); (root/other/'rlog.zst').write_bytes(b'R'*1024)
    receivers = [selected_hold() for _ in Kind]; fixtures = [Fixture(kind, current) for kind in Kind]
    records = []
    try:
        for fixture, receiver in zip(fixtures, receivers, strict=True): await fixture.start(root, receiver)
        for fixture, receiver in zip(fixtures, receivers, strict=True):
            started = await fixture.fetch(Call('/api/dashcam/upload/start', {'segment': segment}))
            assert started['status'] == 200 and started['payload']['ok']
            identifier = started['payload']['job_id']
            await held_count(receiver, 1)
            sync = await fixture.fetch(Call(body={'segment': other}))
            assert sync['status'] == 200 and sync['payload']['ok']
            snapshot = await fixture.fetch(Call('/api/dashcam/upload/job?id='+identifier, method='GET'))
            assert snapshot['payload']['status'] == 'running' and not receiver.eof_before_release.is_set()
            cancelled = await fixture.fetch(Call('/api/dashcam/upload/cancel', {'id': identifier}))
            assert cancelled['status'] == 200 and cancelled['payload']['ok']
            identities = children(fixture.peer.process.pid) if fixture.kind == Kind.NATIVE else {}
            receiver.release.set()
            with anyio.fail_after(4):
                while not exited(identities): await anyio.sleep(.01)
            records.append({'kind': fixture.kind, 'start': started, 'sync': sync, 'job_while_sync_done': snapshot,
                            'cancel': cancelled, 'owned_workers_reaped': exited(identities)})
        compare(records[0]['start'], records[1]['start'], tuple(receivers))
        compare(records[0]['sync'], records[1]['sync'], tuple(receivers))
        compare(records[0]['cancel'], records[1]['cancel'], tuple(receivers))
    finally:
        save(output/'result.json', records)
        for kind, receiver in zip(Kind, receivers, strict=True): save(output/f'{kind}-receiver.json', receiver.captures)
        await close(fixtures, receivers)

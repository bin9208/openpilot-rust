# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# New App smoke, packaging isolation and the single existing 60-second deadline.
from __future__ import annotations

from dataclasses import replace
import json
import os
from pathlib import Path
import select
import time

import anyio
from carrot_server_dashcam_sync_fixtures import Call, Config, Fixture, Kind, children, close, exited
from carrot_server_dashcam_sync_peer import HeldReceiver
from carrot_server_dashcam_sync_probe import files
from carrot_server_dashcam_upload import save
from check_dashcam_runtime import normalize


async def smoke(config: Config, source_capture: Path) -> None:
    output = config.output/'app-smoke'; output.mkdir()
    root, segment = files(output); receiver = HeldReceiver(False)
    fixture = Fixture(Kind.NATIVE, replace(config, output=output, composed=True))
    responses = []
    try:
        await fixture.start(root, receiver)
        responses.append(await fixture.fetch(Call('/api/params_bulk?names=OwnedSyncProbe', method='GET')))
        assert responses[-1]['status'] == 200
        response = await fixture.fetch(Call(body={'segment': segment})); responses.append(response)
        source = json.loads((source_capture/'responses.json').read_text())[-1]['source']
        source_base = json.loads((source_capture/'source/invocation.json').read_text())['providers']['CARROT_WEB_UPLOAD_URL']
        assert response['status'] == 200 and normalize(response['payload'], receiver.base) == normalize(source['payload'], source_base)
        responses.append(await fixture.fetch(Call('/api/params_bulk?names=OwnedSyncProbe', method='GET')))
        assert responses[-1]['status'] == 200 and len(receiver.captures) == 4
    finally:
        save(output/'responses.json', responses); save(output/'receiver.json', receiver.captures)
        await close([fixture], [receiver])


async def packaging(config: Config) -> None:
    output = config.output/'missing-helper'; output.mkdir()
    root, segment = files(output); receiver = HeldReceiver(False)
    fixture = Fixture(Kind.NATIVE, replace(config, output=output, worker=output/'absent-worker', composed=True))
    responses = []
    try:
        await fixture.start(root, receiver)
        responses.append(await fixture.fetch(Call('/api/params_bulk?names=OwnedSyncProbe', method='GET')))
        assert responses[-1]['status'] == 200
        responses.append(await fixture.fetch(Call(body={'segment': segment})))
        assert responses[-1]['status'] == 500 and responses[-1]['payload']['ok'] is False
        responses.append(await fixture.fetch(Call('/api/params_bulk?names=OwnedSyncProbe', method='GET')))
        assert responses[-1]['status'] == 200 and not receiver.captures
    finally:
        save(output/'responses.json', responses); save(output/'receiver.json', receiver.captures)
        await close([fixture], [receiver])


async def grace_expiry(config: Config) -> None:
    output = config.output/'grace-expiry'; output.mkdir()
    root, segment = files(output); receiver = HeldReceiver()
    fixture = Fixture(Kind.NATIVE, replace(config, output=output, composed=True))
    stream = None; fds = []; result = {}
    try:
        await fixture.start(root, receiver)
        assert (await fixture.fetch(Call('/api/params_bulk?names=OwnedSyncProbe', method='GET')))['status'] == 200
        stream = await fixture.connect(Call(body={'segment': segment}))
        assert await anyio.to_thread.run_sync(receiver.started.wait, 4)
        identities = children(fixture.peer.process.pid); assert len(identities) == 1
        fds = [os.pidfd_open(pid) for pid in identities]
        await stream.aclose(); stream = None
        started = time.monotonic(); await fixture.peer.stop()
        samples = []
        with anyio.fail_after(66):
            while fixture.peer.process.returncode is None:
                samples.append({'seconds': time.monotonic()-started, 'worker_reaped': exited(identities),
                                'recipient_eof': receiver.eof_before_release.is_set()})
                await anyio.sleep(.25)
        elapsed = time.monotonic()-started
        result = {'seconds': elapsed, 'server_exit': fixture.peer.process.returncode, 'workers': identities,
                  'workers_reaped': exited(identities), 'pidfds_readable': [bool(select.select([fd], [], [], 0)[0]) for fd in fds],
                  'recipient_eof': await anyio.to_thread.run_sync(receiver.disconnected.wait, 1),
                  'recipient_requests': receiver.captures, 'samples': samples,
                  'scope': 'new native shared-grace ownership check; existing original short lifetime controls reused'}
        assert 59 <= elapsed <= 65 and result['server_exit'] == 0 and result['workers_reaped']
        assert all(result['pidfds_readable']) and result['recipient_eof'] and len(receiver.captures) == 1
        assert all(not sample['worker_reaped'] and not sample['recipient_eof'] for sample in samples if sample['seconds'] < 59)
    finally:
        for fd in fds: os.close(fd)
        if stream: await stream.aclose()
        save(output/'result.json', result)
        await close([fixture], [receiver])

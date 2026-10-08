# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Reuses a caller-supplied source capture or runs the same original three controls.
from __future__ import annotations

import base64
from dataclasses import replace
import json
from pathlib import Path

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_fixtures import Call, Config, Fixture, Kind, children, close, exited
from carrot_server_dashcam_sync_peer import HeldReceiver
from carrot_server_dashcam_sync_probe import files, lifetime as original_lifetime
from carrot_server_dashcam_upload import save
from check_dashcam_runtime import normalize


async def lifetimes(config: Config, references: Path | None) -> None:
    rows = []
    for name in ('disconnect-continue', 'stop-connected', 'disconnect-then-stop'):
        output = config.output/name; output.mkdir()
        if references is None:
            reference = output/'source-replay'
            await original_lifetime(name, reference, config.repository)
        else:
            reference = references/name
        source = json.loads((reference/'result.json').read_text())
        assert source['server_alive_while_held'] and not source['recipient_eof_before_release']
        assert source['server_exit'] == 0 and source['handler']['status'] == 200
        source_base = json.loads((reference/'source/invocation.json').read_text())['providers']['CARROT_WEB_UPLOAD_URL']
        root, segment = files(output)
        receiver = HeldReceiver(); fixture = Fixture(Kind.NATIVE, replace(config, output=output))
        stream = None; observation = {}
        try:
            await fixture.start(root, receiver)
            stream = await fixture.connect(Call(body={'segment': segment}))
            assert await anyio.to_thread.run_sync(receiver.started.wait, 4)
            identities = children(fixture.peer.process.pid); assert len(identities) == 1
            disconnected = name != 'stop-connected'; stopped = name != 'disconnect-continue'
            if disconnected: await stream.aclose(); stream = None
            if stopped: await fixture.peer.stop()
            await anyio.sleep(.25)
            observation = {'server_alive_while_held': fixture.peer.process.returncode is None,
                           'recipient_eof_before_release': receiver.eof_before_release.is_set(),
                           'owned_workers_while_held': identities}
            assert observation['server_alive_while_held'] and not observation['recipient_eof_before_release']
            assert not exited(identities)
            receiver.release.set()
            if stream:
                with anyio.fail_after(5): response = await read_response(BufferedByteReceiveStream(stream), 'POST')
                response['payload'] = json.loads(base64.b64decode(response['body_base64']))
                assert response['status'] == source['http_response']['status'] == 200
                assert normalize(response['payload'], receiver.base) == normalize(source['http_response']['payload'], source_base)
                observation['http_response'] = response
            if not stopped: await fixture.peer.stop()
            with anyio.fail_after(5): await fixture.peer.process.wait()
            observation.update(server_exit=fixture.peer.process.returncode, owned_workers_reaped=exited(identities),
                               recipient_eof=await anyio.to_thread.run_sync(receiver.disconnected.wait, 1))
            assert observation['server_exit'] == 0 and observation['owned_workers_reaped'] and observation['recipient_eof']
            source_requests = json.loads((reference/'receiver.json').read_text())
            assert normalize(source_requests, source_base) == normalize(receiver.captures, receiver.base)
            rows.append({'scenario': name, 'source_reference': str(reference), 'native_requests': len(receiver.captures), 'differences': 0})
        finally:
            if stream: await stream.aclose()
            save(output/'result.json', observation); save(output/'receiver.json', receiver.captures)
            await close([fixture], [receiver])
    save(config.output/'comparison.json', {'cases': rows, 'paired_lifetimes': 3, 'differences': 0})

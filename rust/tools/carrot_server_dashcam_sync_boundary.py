# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Called by the sync HTTP oracle; compares only the new route boundary.
from __future__ import annotations

from dataclasses import replace
import hashlib
import json
from pathlib import Path

import anyio
from carrot_server_dashcam_sync_fixtures import Call, Config, Fixture, Kind, close, compare
from carrot_server_dashcam_sync_probe import files
from carrot_server_dashcam_upload import save
from check_dashcam_runtime import Receiver, normalize


def session_error_receiver() -> Receiver:
    receiver = Receiver('normal')
    handler = receiver.server.RequestHandlerClass
    original = handler.do_POST
    def post(peer):
        if peer.path.endswith('/session'):
            body = peer.body()
            receiver.captures.append({'method': 'POST', 'path': peer.path, 'body': json.loads(body), 'auth': peer.headers.get('Authorization')})
            peer.reply(403, {'ok': False, 'error': 'owned session rejection'})
        else:
            original(peer)
    handler.do_POST = post
    return receiver


async def boundary(config: Config) -> None:
    rows = []
    for scenario in ('normal', 'partial', 'session-error'):
        output = config.output/scenario; output.mkdir()
        current = replace(config, output=output, token='' if scenario == 'session-error' else config.token)
        root, segment = files(output)
        if scenario == 'partial':
            (root/segment).rename(root/'00000001--1234567890--1'); segment = '00000001--1234567890--1'
        receivers = [session_error_receiver() for _ in Kind] if scenario == 'session-error' else [Receiver(scenario) for _ in Kind]
        fixtures = [Fixture(kind, current) for kind in Kind]
        responses = []
        try:
            for fixture, receiver in zip(fixtures, receivers, strict=True): await fixture.start(root, receiver)
            calls = [Call(body={'segment': segment})]
            if scenario == 'normal':
                calls = [Call(method='GET'), Call(method='HEAD'), Call(body={}),
                         Call(body={'segment': '00000001--1234567890--99'}), Call('/api/dashcam/%75pload', {'segment': segment})]
            for call in calls:
                source = await fixtures[0].fetch(call); native = await fixtures[1].fetch(call)
                responses.append({'call': {'method': call.method, 'path': call.path, 'body': call.body}, 'source': source, 'native': native})
                compare(source, native, tuple(receivers))
            final = responses[-1]
            assert final['source']['status'] == (500 if scenario == 'session-error' else 200)
            if scenario == 'partial': assert not final['source']['payload']['ok']
            if scenario == 'normal':
                assert final['source']['payload']['ok']
                expected = {(4096, hashlib.sha256(b'Q'*4096).hexdigest()), (1024, hashlib.sha256(b'R'*1024).hexdigest())}
                for receiver in receivers:
                    assert len(receiver.captures) == 4
                    assert {(row['size'], row['sha256']) for row in receiver.captures if row['method'] == 'PUT'} == expected
            if scenario == 'session-error':
                for receiver in receivers:
                    assert len(receiver.captures) == 1 and receiver.captures[0]['body']['purpose'] == 'dashcam'
            rows.append({'scenario': scenario, 'paired_responses': len(responses), 'differences': 0,
                         'source_requests': len(receivers[0].captures), 'native_requests': len(receivers[1].captures)})
        finally:
            save(output/'responses.json', responses)
            for kind, receiver in zip(Kind, receivers, strict=True): save(output/f'{kind}-receiver.json', receiver.captures)
            await close(fixtures, receivers)
    save(config.output/'comparison.json', {'cases': rows, 'paired_responses': sum(row['paired_responses'] for row in rows),
         'differences': 0, 'reuse': 'unchanged52-job/catalog/engine/report/transport and shared request decoder proofs',
         'normalization': 'existing upload normalizer; ownedURL/uploadedAt/shareTextTime. Deterministic aggregate/result/meta fields preserved.'})

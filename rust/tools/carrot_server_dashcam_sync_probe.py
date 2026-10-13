#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run from the repository root with the existing source dependencies.
# python rust/tools/carrot_server_dashcam_sync_probe.py --worker PATH --output NEW_DIR
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from collections.abc import Awaitable
from typing import TypeAlias, TypedDict

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_health import repository
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_peer import HeldReceiver
from carrot_server_dashcam_upload import save
from check_dashcam_runtime import normalize

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


class ControlReply(TypedDict):
    jobs: list[str]


class SourceReturn(TypedDict, total=False):
    status: int
    payload: dict[str, Json]
    cancelled: bool


def environment(output: Path, recipient: str, repo: Path) -> dict[str, str]:
    state = output/'data'; (state/'state').mkdir(parents=True)
    (state/'state/web_settings.json').write_text(json.dumps({'web_upload_url': recipient}))
    params = output/'params'; namespace = params/'owned-sync'; namespace.mkdir(parents=True)
    (namespace/'CarName').write_text('owned fixture'); (namespace/'DongleId').write_text('owned-sync-device')
    return {**os.environ, 'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': 'owned-sync',
            'CARROT_DATA_DIR': str(state), 'CARROT_REPO_DIR': str(repo),
            'CARROT_WEB_UPLOAD_URL': recipient, 'CARROT_WEB_UPLOAD_TOKEN': 'owned-static-token',
            'CARROT_DEVICE_SERIAL': 'owned-sync-serial', 'DEVICE_SERIAL': '', 'SERIAL': '',
            'CARROT_WEB_UPLOAD_CONCURRENCY': '1', 'CARROT_DISCORD_WEBHOOK_URL': recipient+'/webhook-old',
            'DISCORD_WEBHOOK_URL': '', 'CARROT_DISCORD_WEBHOOK_DISABLE': '0',
            'CARROT_TMUX_WEB_UPLOAD_URL': recipient+'/unused-tmux'}


def files(output: Path) -> tuple[Path, str]:
    root = output/'owned-root'; segment = '00000001--1234567890--0'
    directory = root/segment; directory.mkdir(parents=True)
    (directory/'qcamera.ts').write_bytes(b'Q'*4096); (directory/'rlog.zst').write_bytes(b'R'*1024)
    return root, segment


class Peer:
    def __init__(self, output: Path):
        self.output = output; self.process = None; self.log = None; self.stopped = False

    async def start(self, command: list[str], config: dict, env: dict, ready: bool) -> None:
        self.log = (self.output/'process.log').open('wb')
        self.process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, env=env)
        self.reader = BufferedByteReceiveStream(self.process.stdout)
        await self.process.stdin.send((json.dumps(config)+'\n').encode())
        if ready:
            with anyio.fail_after(5):
                self.ready = json.loads(await self.reader.receive_until(b'\n', 65536))
        save(self.output/'invocation.json', {'command': command, 'config': config, 'pid': self.process.pid,
             'providers': {key: value for key, value in env.items() if isinstance(key, str) and key in {'PARAMS_ROOT','OPENPILOT_PREFIX','CARROT_DATA_DIR','CARROT_REPO_DIR','CARROT_WEB_UPLOAD_URL','CARROT_DISCORD_WEBHOOK_URL'}}})

    async def control(self, value: dict) -> ControlReply:
        await self.process.stdin.send((json.dumps(value)+'\n').encode())
        with anyio.fail_after(3):
            return json.loads(await self.reader.receive_until(b'\n', 65536))

    async def stop(self) -> None:
        if not self.stopped and self.process.returncode is None:
            await self.process.stdin.send(b'\n'); await self.process.stdin.aclose()
            self.stopped = True

    async def close(self, expected: int = 0) -> None:
        errors = []
        if self.process:
            try:
                await self.stop()
                with anyio.fail_after(6): await self.process.wait()
            except (anyio.BrokenResourceError, anyio.ClosedResourceError, BrokenPipeError, TimeoutError) as error:
                errors.append(f'{type(error).__name__}: {error}')
                if self.process.returncode is None:
                    self.process.kill()
                    with anyio.fail_after(3): await self.process.wait()
            finally:
                await self.process.aclose()
        if self.log: self.log.close()
        save(self.output/'cleanup.json', {'exit': self.process.returncode if self.process else None, 'expected': expected, 'errors': errors})
        assert not errors and (self.process is None or self.process.returncode == expected)


async def startup(peer: Peer, operation: Awaitable[None]) -> None:
    try:
        await operation
    except BaseException:  # noqa: BROAD_EXCEPT_OK - failed readiness must release its inaccessible owned process, including cancellation.
        with anyio.CancelScope(shield=True):
            try:
                await peer.close()
            except BaseException as error:  # noqa: BROAD_EXCEPT_OK - record cleanup failure while preserving the original startup exception.
                save(peer.output/'startup-cleanup-error.json', {'type': type(error).__name__, 'error': str(error)})
        raise


async def source_peer(output: Path, receiver: HeldReceiver, repo: Path, root: Path, non_utf8: bool = False) -> Peer:
    output.mkdir()
    env = environment(output, receiver.base, repo)
    if non_utf8:
        env[b'OWNED_UNRELATED_NON_UTF8'] = b'\xff'
    peer = Peer(output)
    config = {'root': str(root), 'state': env['CARROT_DATA_DIR'], 'output': str(output)}
    await startup(peer, peer.start([sys.executable, '-P', str(Path(__file__).with_name('carrot_server_dashcam_sync_source.py'))], config, env, True))
    return peer


async def send(port: int, segment: str):
    stream = await anyio.connect_tcp('127.0.0.1', port)
    body = json.dumps({'segment': segment}).encode()
    await stream.send(f'POST /api/dashcam/upload HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {len(body)}\r\n\r\n'.encode()+body)
    return stream


async def returned(output: Path) -> SourceReturn:
    with anyio.fail_after(5):
        while not (output/'handler-return.json').exists() and not (output/'handler-cancelled.json').exists():
            await anyio.sleep(.01)
    path = output/('handler-return.json' if (output/'handler-return.json').exists() else 'handler-cancelled.json')
    return json.loads(path.read_text())


async def cleanup(peers: list[tuple[Peer, int]], receivers: list[HeldReceiver], stream) -> list[str]:
    errors = []
    for receiver in receivers: receiver.release.set()
    if stream:
        try: await stream.aclose()
        except (anyio.BrokenResourceError, anyio.ClosedResourceError, OSError) as error: errors.append(str(error))
    for peer, expected in peers:
        try: await peer.close(expected)
        except (AssertionError, OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error: errors.append(str(error))
    for receiver in receivers:
        try: await anyio.to_thread.run_sync(receiver.close)
        except (AssertionError, OSError) as error: errors.append(str(error))
    return errors


async def lifetime(name: str, output: Path, repo: Path) -> None:
    output.mkdir(); root, segment = files(output); receiver = HeldReceiver(); peer = None; stream = None
    observation = {}
    try:
        peer = await source_peer(output/'source', receiver, repo, root)
        stream = await send(peer.ready['port'], segment)
        assert await anyio.to_thread.run_sync(receiver.started.wait, 3)
        disconnected = name != 'stop-connected'
        stopped = name != 'disconnect-continue'
        if disconnected: await stream.aclose(); stream = None
        if stopped: await peer.stop()
        else: observation['jobs_while_held'] = await peer.control({'op': 'jobs'})
        await anyio.sleep(.25)
        observation.update(server_alive_while_held=peer.process.returncode is None, recipient_eof_before_release=receiver.eof_before_release.is_set(),
                           handler_returned_before_release=(peer.output/'handler-return.json').exists(), handler_cancelled_before_release=(peer.output/'handler-cancelled.json').exists())
        receiver.release.set()
        if stream:
            with anyio.fail_after(5): response = await read_response(BufferedByteReceiveStream(stream), 'POST')
            response['payload'] = json.loads(base64.b64decode(response['body_base64']))
            observation['http_response'] = response
        observation['handler'] = await returned(peer.output)
        if not stopped: await peer.stop()
        with anyio.fail_after(5): await peer.process.wait()
        observation.update(server_exit=peer.process.returncode, recipient_eof=await anyio.to_thread.run_sync(receiver.disconnected.wait, 1), jobs_at_exit=json.loads((peer.output/'jobs-at-exit.json').read_text()))
    finally:
        errors = await cleanup([(peer, 0)] if peer else [], [receiver], stream)
        observation['cleanup_errors'] = errors
        save(output/'receiver.json', receiver.captures); save(output/'result.json', observation)
        assert not errors


async def packets(peer: Peer, rows: list) -> None:
    try:
        while True:
            rows.append(json.loads(await peer.reader.receive_until(b'\n', 1024*1024)))
    except (anyio.EndOfStream, anyio.IncompleteRead):
        assert not peer.reader.buffer
        return


async def worker_gap(name: str, output: Path, repo: Path, worker: Path, env_fixed: bool) -> None:
    output.mkdir(); root, segment = files(output); source_receiver = HeldReceiver(name == 'lookup-timing'); native_receiver = HeldReceiver(name == 'lookup-timing')
    source = None; native = Peer(output/'native'); stream = None; native_rows = []; observation = {}
    native.output.mkdir()
    try:
        source = await source_peer(output/'source', source_receiver, repo, root, name == 'non-utf8')
        env = environment(native.output, native_receiver.base, repo)
        if name == 'non-utf8':
            env[b'OWNED_UNRELATED_NON_UTF8'] = b'\xff'
        await native.start([str(worker), '--worker'], {'parent_pid': os.getpid(), 'root': str(root), 'id': 'owned-sync', 'segments': [segment], 'settings': None}, env, False)
        stream = await send(source.ready['port'], segment)
        async with anyio.create_task_group() as tasks:
            tasks.start_soon(packets, native, native_rows)
            if name == 'lookup-timing':
                assert await anyio.to_thread.run_sync(source_receiver.started.wait, 3)
                assert await anyio.to_thread.run_sync(native_receiver.started.wait, 3)
                observation['source_webhook_lookup_before_release'] = (source.output/'webhook-lookup.json').exists()
                observation['native_context_before_release'] = any(row['event'] == 'context' for row in native_rows)
                source_receiver.release.set(); native_receiver.release.set()
            with anyio.fail_after(8):
                response = await read_response(BufferedByteReceiveStream(stream), 'POST')
                await native.process.wait()
            response['payload'] = json.loads(base64.b64decode(response['body_base64']))
            observation.update(source_response=response, native_packets=native_rows, native_exit=native.process.returncode)
        observation['native_packets'] = native_rows
        observation['source_handler'] = await returned(source.output)
        if name == 'lookup-timing':
            observation['source_webhook_lookup'] = json.loads((source.output/'webhook-lookup.json').read_text())
            observation['limitation'] = 'immutable env/default; no supported dynamic provider found. Timing order is recorded; no output mismatch or extra worker IPC justified.'
    finally:
        peers = ([(source, 0)] if source else [])+[(native, 101 if name == 'non-utf8' and not env_fixed else 0)]
        errors = await cleanup(peers, [source_receiver, native_receiver], stream)
        observation['cleanup_errors'] = errors
        save(output/'source-receiver.json', source_receiver.captures); save(output/'native-receiver.json', native_receiver.captures); save(output/'result.json', observation)
        assert not errors


def compare_worker_case(output: Path) -> None:
    result = json.loads((output/'result.json').read_text())
    finish = [row['patch'] for row in result['native_packets'] if row['event'] == 'finish']
    assert result['native_exit'] == 0 and len(finish) == 1 and finish[0]['ok']
    source_base = json.loads((output/'source/invocation.json').read_text())['providers']['CARROT_WEB_UPLOAD_URL']
    native_base = json.loads((output/'native/invocation.json').read_text())['providers']['CARROT_WEB_UPLOAD_URL']
    equal = normalize(result['source_response']['payload'], source_base) == normalize(finish[0]['result'], native_base)
    assert result['source_response']['status'] == 200 and equal
    counts = []
    for peer in ('source', 'native'):
        requests = json.loads((output/f'{peer}-receiver.json').read_text())
        puts = {(row['size'], row['sha256']) for row in requests if row['method'] == 'PUT'}
        assert puts == {(4096, hashlib.sha256(b'Q'*4096).hexdigest()), (1024, hashlib.sha256(b'R'*1024).hexdigest())}
        assert len(requests) == 4
        counts.append(len(requests))
    save(output/'comparison.json', {'source_status': 200, 'native_exit': 0, 'payload_equal': equal, 'request_counts': counts})


async def partial_start(output: Path) -> None:
    output.mkdir(); peer = Peer(output)
    before = len(list(Path('/proc/self/fd').iterdir()))
    command = [sys.executable, '-c', "import threading; print('malformed-ready', flush=True); threading.Event().wait()"]
    try:
        await startup(peer, peer.start(command, {}, os.environ.copy(), True))
    except json.JSONDecodeError as error:
        original = type(error).__name__
    else:
        raise AssertionError('malformed readiness did not fail')
    after = len(list(Path('/proc/self/fd').iterdir()))
    result = {'command': command, 'original_error': original, 'pid': peer.process.pid, 'exit': peer.process.returncode,
              'child_reaped': not Path(f'/proc/{peer.process.pid}').exists(), 'fd_before': before, 'fd_after': after,
              'cleanup': json.loads((output/'cleanup.json').read_text())}
    save(output/'partial-start.json', result)
    assert original == 'JSONDecodeError' and result['child_reaped'] and result['exit'] == -9 and before == after


async def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument('--worker', type=Path, required=True); parser.add_argument('--output', type=Path, required=True); parser.add_argument('--worker-controls', action='store_true'); parser.add_argument('--expect-env-fixed', action='store_true'); parser.add_argument('--partial-start-check', action='store_true')
    args = parser.parse_args(); output = args.output.resolve(); output.mkdir(parents=True); worker = args.worker.resolve()
    repo = output/'owned-repository'; repository(repo)
    save(output/'invocation.json', {'command': [sys.executable, '-P', *sys.argv], 'worker': str(worker), 'sha256': hashlib.sha256(worker.read_bytes()).hexdigest(), 'scope': 'original3 held-PUT lifetimes plus2 worker provider gaps; no adapter/production edits'})
    if args.partial_start_check:
        await partial_start(output/'partial-start'); return
    if not args.worker_controls:
        for name in ('disconnect-continue', 'stop-connected', 'disconnect-then-stop'): await lifetime(name, output/name, repo)
    for name in ('non-utf8', 'lookup-timing'):
        await worker_gap(name, output/name, repo, worker, args.expect_env_fixed)
        if args.expect_env_fixed or name == 'lookup-timing': compare_worker_case(output/name)
    print(json.dumps({'recorded': 2 if args.worker_controls else 5, 'output': str(output)}))


if __name__ == '__main__':
    anyio.run(main)

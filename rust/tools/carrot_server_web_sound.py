# /// script
# requires-python = ">=3.12"
# dependencies = ["anyio", "aiohttp", "pycapnp==2.1.0"]
# ///
from __future__ import annotations

import argparse
from dataclasses import dataclass
from functools import partial
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid
from typing import assert_never

import anyio
from aiohttp import ClientSession, WSMsgType, web
from card_runtime_source import load_binding

type Json = None | bool | int | float | str | list[Json] | dict[str, Json]


@dataclass(frozen=True, slots=True)
class ProbeFailure(Exception):
    detail: str

    def __str__(self) -> str:
        return self.detail


async def source_server() -> None:
    path = await anyio.to_thread.run_sync(Path('openpilot/selfdrive/carrot/server/features/web_sound.py').resolve)
    spec = importlib.util.spec_from_file_location('original_web_sound', path)
    if spec is None or spec.loader is None:
        raise ProbeFailure('source feature unavailable')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    app = web.Application()
    module.register(app)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    try:
        print(json.dumps({'port': site._server.sockets[0].getsockname()[1]}), flush=True)
        await anyio.to_thread.run_sync(sys.stdin.readline)
    finally:
        await runner.cleanup()


async def state(socket) -> str:
    while True:
        message = await socket.receive(timeout=18)
        match message.type:
            case WSMsgType.PING:
                await socket.pong(message.data)
            case WSMsgType.TEXT:
                return message.data
            case WSMsgType.BINARY | WSMsgType.CONTINUATION | WSMsgType.CLOSE | WSMsgType.CLOSING | WSMsgType.CLOSED | WSMsgType.ERROR | WSMsgType.PONG:
                raise ProbeFailure(f'unexpected state receive {message.type.name}: {message.data!r}')
            case _:
                assert_never(message.type)


async def protocol(client: ClientSession, url: str) -> list[Json]:
    rows: list[Json] = []
    for name, payload in (('custom-close', b'\x0b\xb9owned close reason'), ('empty-close', b'')):
        async with client.ws_connect(url, autoping=False, autoclose=False, compress=15) as socket:
            initial = await state(socket)
            await socket.send_frame(payload, WSMsgType.CLOSE)
            message = await socket.receive(timeout=3)
            rows.append({'case': name, 'initial': initial, 'close_code': message.data,
                         'type': message.type.name, 'reason': message.extra,
                         'compression': socket.compress, 'extension': socket._response.headers.get('Sec-WebSocket-Extensions')})
    for binary in (False, True):
        for size in (65535, 65536):
            async with client.ws_connect(url, autoping=False) as socket:
                await state(socket)
                if binary:
                    await socket.send_bytes(b'x' * size)
                else:
                    await socket.send_str('x' * size)
                await socket.ping(b'owned-marker')
                message = await socket.receive(timeout=3)
                rows.append({'case': 'binary' if binary else 'text', 'size': size,
                             'type': message.type.name, 'data': message.data.hex() if isinstance(message.data, bytes) else message.data})
    return rows


async def ipc_states(client: ClientSession, url: str, root: Path) -> tuple[list[str], list[Json]]:
    from openpilot.cereal import messaging
    from openpilot.common.params import Params
    params = Params(str(root))
    publisher = messaging.PubMaster(['selfdriveState', 'carrotMan', 'carState'])
    def publish(name: str, values: dict[str, Json], valid: bool = True) -> None:
        message = messaging.new_message(name, valid=valid)
        payload = getattr(message, name)
        for key, value in values.items():
            setattr(payload, key, value)
        publisher.send(name, message)
    rows: list[str] = []
    timing: list[Json] = []
    async with client.ws_connect(url, autoping=False) as socket:
        rows.append(await state(socket))
        publish('selfdriveState', {'enabled': False, 'alertSound': 'engage'})
        rows.append(await state(socket))
        publish('selfdriveState', {'enabled': False, 'alertSound': 'engage'})
        with anyio.move_on_after(.15) as silence:
            unexpected = await state(socket)
            raise AssertionError(f'unchanged state emitted {unexpected}')
        assert silence.cancel_called
        publish('carrotMan', {'leftSec': 3})
        rows.append(await state(socket))
        publish('selfdriveState', {'alertSound': 'disengage'}, False)
        rows.append(await state(socket))
        publish('selfdriveState', {'enabled': False, 'alertSound': 'disengage'})
        rows.append(await state(socket))
        publish('carrotMan', {'leftSec': 2}, False)
        rows.append(await state(socket))
        publish('carrotMan', {'leftSec': 2})
        rows.append(await state(socket))
        for key, value in {'SoundVolumeAdjust': b'250', 'SoundVolumeAdjustEngage': b'-5', 'SoundLanguageSetting': b' ko_KR '}.items():
            await anyio.to_thread.run_sync(Path(params.get_param_path(key)).write_bytes, value)
        rows.append(await state(socket))
        publish('carState', {'buttonEvents': [{'type': 'mainCruise', 'pressed': True}]})
        rows.append(await state(socket))
        rows.append(await state(socket))
        publish('carState', {'buttonEvents': [{'type': 'mainCruise', 'pressed': False}]})
        before = time.monotonic()
        publish('selfdriveState', {'enabled': True, 'alertSound': 'engage'})
        rows.append(await state(socket))
        rows.append(await state(socket))
        timing.append({'case': 'missing-warning', 'elapsed': time.monotonic() - before})
        rows.append(await state(socket))
        timing.append({'case': 'missing-clear', 'elapsed': time.monotonic() - before})
        publish('selfdriveState', {'enabled': True, 'alertSound': 'disengage'})
        rows.append(await state(socket))
    return rows, timing


async def heartbeat(client: ClientSession, url: str) -> tuple[list[Json], list[Json]]:
    rows: list[Json] = []
    timing: list[Json] = []
    async with client.ws_connect(url, autoping=False) as socket:
        await state(socket)
        before = time.monotonic()
        message = await socket.receive(timeout=22)
        assert message.type == WSMsgType.PING and message.data == b''
        timing.append({'case': 'heartbeat', 'elapsed': time.monotonic() - before})
        rows.append({'case': 'heartbeat', 'type': message.type.name, 'data': message.data.hex()})
        before = time.monotonic()
        message = await socket.receive(timeout=12)
        timing.append({'case': 'pong-timeout', 'elapsed': time.monotonic() - before})
        rows.append({'case': 'pong-timeout', 'type': message.type.name, 'close_code': socket.close_code})
    async with client.ws_connect(url, autoping=False) as socket:
        await state(socket)
        await anyio.sleep(1.2)
        before = time.monotonic()
        await socket.send_str('owned heartbeat activity reset')
        message = await socket.receive(timeout=22)
        assert message.type == WSMsgType.PING and message.data == b''
        timing.append({'case': 'activity-reset', 'elapsed': time.monotonic() - before})
        await socket.send_bytes(b'owned activity replaces pong')
        with anyio.move_on_after(12.2) as silence:
            message = await socket.receive(timeout=13)
            raise AssertionError(f'activity failed to reset heartbeat {message.type.name}')
        assert silence.cancel_called
        await socket.ping(b'owned-after-reset')
        message = await socket.receive(timeout=3)
        rows.append({'case': 'activity-reset', 'type': message.type.name, 'data': message.data.hex()})
    return rows, timing


async def failed_sender(client: ClientSession, url: str) -> list[Json]:
    async with client.ws_connect(url, autoping=False) as socket:
        with anyio.move_on_after(.2) as silence:
            message = await socket.receive(timeout=1)
            raise AssertionError(f'failed sender emitted {message.type.name}')
        assert silence.cancel_called
        await socket.ping(b'owned-sender-failed')
        message = await socket.receive(timeout=3)
        return [{'case': 'sender-init-failed-receiver-live', 'type': message.type.name, 'data': message.data.hex(),
                 'status': socket._response.status}]


async def main() -> None:
    parser = argparse.ArgumentParser()
    for flag in ('--binding', '--binary', '--output', '--source-capture'):
        parser.add_argument(flag, type=Path, required=flag in ('--binding', '--output'))
    parser.add_argument('--sender-failure-only', nargs='?', const='ipc', choices=('ipc', 'params'))
    for flag in ('--source', '--protocol-only', '--heartbeat-only'):
        parser.add_argument(flag, action='store_true')
    args = parser.parse_args()
    binding = await anyio.to_thread.run_sync(args.binding.resolve)
    load_binding(binding)
    if args.source:
        await source_server()
        return
    await anyio.Path(args.output).mkdir(parents=True, exist_ok=True)
    results: dict[str, Json] = {}
    if args.source_capture:
        results['original'] = json.loads(await anyio.Path(args.source_capture / 'original/observations.json').read_text())
    invocations: list[Json] = []
    for side in ('original', 'native'):
        if side == 'original' and args.source_capture:
            continue
        if side == 'native' and args.binary is None:
            continue
        root = await anyio.to_thread.run_sync((args.output / side).resolve)
        await anyio.Path(root).mkdir(exist_ok=True)
        prefix = 'websound_' + uuid.uuid4().hex
        os.environ['OPENPILOT_PREFIX'] = prefix
        os.environ['PARAMS_ROOT'] = str(root / 'params')
        ipc = Path('/dev/shm') / f'msgq_{prefix}'
        if args.sender_failure_only != 'ipc':
            await anyio.Path(ipc).mkdir()
        paths = {key: str(root / key) for key in ('repository', 'data', 'settings', 'web', 'shared_assets', 'training_assets', 'legacy_state', 'params')}
        for key, value in paths.items():
            if key == 'params' and args.sender_failure_only == 'params':
                await anyio.Path(value).write_text('owned non-directory Params root\n')
            elif key != 'settings':
                await anyio.Path(value).mkdir(parents=True, exist_ok=True)
        await anyio.Path(paths['settings']).write_text('{"params":[]}\n')
        helper = await anyio.to_thread.run_sync(Path(__file__).resolve)
        command = ([sys.executable, '-P', str(helper), '--source', '--binding', str(binding), '--output', str(root)]
                   if side == 'original' else [str(await anyio.to_thread.run_sync(args.binary.resolve))])
        invocations.append({'side': side, 'command': command, 'params': paths['params'], 'prefix': prefix, 'ipc': str(ipc), 'config': paths})
        await anyio.Path(args.output / 'invocations.json').write_text(json.dumps(invocations, indent=2) + '\n')
        with await anyio.to_thread.run_sync((root / 'stderr.log').open, 'w') as errors:
            process = await anyio.to_thread.run_sync(partial(subprocess.Popen, command, stdin=subprocess.PIPE,
                                                           stdout=subprocess.PIPE, stderr=errors, text=True))
            assert process.stdin is not None and process.stdout is not None
            if side == 'native':
                process.stdin.write(json.dumps(paths | {'unavailable': args.sender_failure_only == 'params'}) + '\n')
                process.stdin.flush()
            try:
                port = json.loads(await anyio.to_thread.run_sync(process.stdout.readline))['port']
                url = f'http://127.0.0.1:{port}/ws/web_sound'
                async with ClientSession() as client:
                    if args.sender_failure_only:
                        rows, states, timing = await failed_sender(client, url), [], []
                    elif args.heartbeat_only:
                        rows, timing = await heartbeat(client, url)
                        states = []
                    else:
                        rows = await protocol(client, url)
                        states, timing = ([], []) if args.protocol_only else await ipc_states(client, url, root / 'params')
                results[side] = {'protocol': rows, 'states': states}
                await anyio.Path(root / 'timing.json').write_text(json.dumps(timing, indent=2) + '\n')
            finally:
                process.stdin.write('stop\n')
                process.stdin.flush()
                process.stdin.close()
                code = await anyio.to_thread.run_sync(partial(process.wait, timeout=10))
                await anyio.Path(root / 'exit.json').write_text(json.dumps({'exit_code': code}) + '\n')
                if args.sender_failure_only != 'ipc':
                    await anyio.to_thread.run_sync(shutil.rmtree, ipc)
                assert code == 0
        await anyio.Path(root / 'observations.json').write_text(json.dumps(results[side], indent=2) + '\n')
    passed = 'native' in results and results['original'] == results['native']
    receipt = {'passed': passed, 'source_only': args.binary is None, 'sides': len(results), 'source_capture': str(args.source_capture),
               'surface': 'actual process/owned native IPC/loopback WebSocket/raw payloads'}
    await anyio.Path(args.output / 'result.json').write_text(json.dumps(receipt) + '\n')
    print(json.dumps(receipt))
    assert args.binary is None or passed


if __name__ == '__main__':
    anyio.run(main, backend='asyncio')

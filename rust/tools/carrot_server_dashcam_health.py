#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run from the repository root with source dependencies supplied by the caller.
# python rust/tools/carrot_server_dashcam_health.py --binary PATH --output NEW_DIR
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import TypeAlias, TypedDict

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import request, save
from carrot_server_dashcam_health_peer import Case, Receiver

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


class Comparison(TypedDict):
    status: int
    headers: dict[str, str]
    payload: Json


class Observation(TypedDict):
    scenario: str
    equal: bool
    rows: list[dict[str, Json]]


def repository(path: Path) -> None:
    path.mkdir()
    (path/'owned.txt').write_text('owned health metadata fixture\n')
    environment = {**os.environ, 'GIT_AUTHOR_NAME': 'Owned Fixture', 'GIT_AUTHOR_EMAIL': 'owned@example.invalid',
                   'GIT_COMMITTER_NAME': 'Owned Fixture', 'GIT_COMMITTER_EMAIL': 'owned@example.invalid',
                   'GIT_AUTHOR_DATE': '2026-09-30T12:00:00+0000', 'GIT_COMMITTER_DATE': '2026-09-30T12:00:00+0000'}
    for args in [('init', '-q', '-b', 'owned-health'), ('add', 'owned.txt'), ('commit', '-q', '-m', 'owned fixture')]:
        subprocess.run(['/usr/bin/git', *args], cwd=path, env=environment, check=True, capture_output=True)


def canonical(response: dict, base: str) -> Comparison:
    payload = response['payload']
    if isinstance(payload, dict):
        payload = dict(payload)
        payload.pop('elapsed_ms', None)
        if payload.get('url') == base:
            payload['url'] = '<owned-recipient>'
    return {'status': response['status'], 'headers': {key: value for key, value in response['headers'].items() if key in ('content-type', 'allow')}, 'payload': payload}


async def run_case(case: Case, binary: Path | None, output: Path, repo: Path, composed: bool) -> Observation:
    output.mkdir()
    receivers = []
    processes = []
    logs = []
    rows = []
    errors = []
    requested_stops = set()
    try:
        for _ in range(2 if binary else 1):
            receivers.append(Receiver(case))
        for side, receiver in enumerate(receivers):
            state = output/('source-state' if side == 0 else 'native-state')
            (state/'state').mkdir(parents=True)
            (state/'state/web_settings.json').write_text(json.dumps({'web_upload_url': receiver.base if not case.environment_url else 'http://127.0.0.1:1'}))
            (state/'settings.json').write_text('{"params":[]}')
            tools = state/'bin'
            tools.mkdir()
            git_log = state/'git.jsonl'
            wrapper = tools/'git'
            wrapper.write_text('#!'+sys.executable+'\nimport json,os,sys\nwith open(os.environ["OWNED_HEALTH_GIT_LOG"],"a") as output: output.write(json.dumps(sys.argv[1:])+"\\n")\nos.execv("/usr/bin/git",["git",*sys.argv[1:]])\n')
            wrapper.chmod(0o755)
            environment = {**os.environ, 'PATH': str(tools)+os.pathsep+os.environ['PATH'], 'CARROT_DATA_DIR': str(state), 'CARROT_REPO_DIR': str(repo),
                           'CARROT_DEVICE_SERIAL': 'owned-health-serial', 'CARROT_WEB_UPLOAD_TOKEN': case.token,
                           'CARROT_WEB_UPLOAD_URL': 'owned-invalid-url' if case.invalid_url else receiver.base if case.environment_url else '',
                           'OWNED_HEALTH_GIT_LOG': str(git_log), 'OPENPILOT_PREFIX': f'owned-health-{os.getpid()}-{side}'}
            if case.unrelated_non_utf8:
                environment[b'OWNED_UNRELATED_NON_UTF8'] = b'\xff'
            command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_dashcam_health_source.py'))] if side == 0 else [str(binary)]
            log = (output/('source.log' if side == 0 else 'native.log')).open('wb')
            logs.append(log)
            process = await anyio.open_process(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, env=environment)
            processes.append(process)
            config = {'state': str(state), 'repository': str(repo), 'output': str(output), 'composed': composed}
            await process.stdin.send((json.dumps(config)+'\n').encode())
            with anyio.fail_after(5):
                ready = json.loads(await BufferedByteReceiveStream(process.stdout).receive_until(b'\n', 65536))
            before = await request(ready['port'], '/api/params_bulk?names=OwnedHealthProbe') if composed and side == 1 else None
            git_before = git_log.read_text().splitlines() if git_log.exists() else []
            assert not git_before
            pending = []
            held_observation = None
            if case.held:
                async def issuing_request() -> None:
                    pending.append(await request(ready['port'], '/api/dashcam/upload/test', case.method))
                async with anyio.create_task_group() as tasks:
                    tasks.start_soon(issuing_request)
                    assert await anyio.to_thread.run_sync(receiver.started.wait, 3)
                    await process.stdin.send(b'\n')
                    await process.stdin.aclose()
                    requested_stops.add(process.pid)
                    await anyio.sleep(.2)
                    held_observation = {'server_alive_while_held_after_stop': process.returncode is None, 'response_pending_while_held': not pending}
                    assert process.returncode is None and not pending
                    receiver.release.set()
                response = pending[0]
                with anyio.fail_after(5):
                    await process.wait()
                assert process.returncode == 0
                assert await anyio.to_thread.run_sync(receiver.disconnected.wait, 3)
                held_observation.update(recipient_eof=True, exit_code=process.returncode)
            else:
                response = await request(ready['port'], '/api/dashcam/upload/test', case.method)
            after = await request(ready['port'], '/api/params_bulk?names=OwnedHealthProbe') if composed and side == 1 and not case.held else None
            git_calls = [json.loads(line) for line in git_log.read_text().splitlines()] if git_log.exists() else []
            rows.append({'side': 'source' if side == 0 else 'native', 'command': command, 'config': config, 'response': response,
                         'recipient': receiver.base, 'recipient_requests': receiver.captures, 'metadata_git_calls': git_calls,
                         'metadata_git_calls_before_request': git_before, 'unrelated_before': before, 'unrelated_after': after, 'held_stop': held_observation})
            needs_session = not case.invalid_url and case.method == 'POST' and case.health_status == 200 and not case.token.strip()
            expected_count = 0 if case.invalid_url or case.method != 'POST' else 2 if needs_session else 1
            assert response['status'] == case.expected_status
            assert len(receiver.captures) == expected_count
            assert len(git_calls) == (3 if needs_session else 0)
            if expected_count:
                assert receiver.captures[0] == {'method': 'GET', 'path': '/api/v1/health', 'authorization': 'Bearer '+case.token.strip() if case.token.strip() else ''}
            if needs_session:
                session = receiver.captures[1]
                assert session['method'] == 'POST' and session['path'] == '/api/v1/session' and not session['authorization']
                assert session['payload']['purpose'] == 'test' and session['payload']['serial'] == 'owned-health-serial'
                assert session['payload']['branch'] == 'owned-health'
            if composed and side == 1:
                assert before['status'] == 200
                if not case.held:
                    assert after['status'] == 200
        equal = len(rows) == 1 or canonical(rows[0]['response'], receivers[0].base) == canonical(rows[1]['response'], receivers[1].base)
        if len(rows) == 2:
            assert rows[0]['recipient_requests'] == rows[1]['recipient_requests']
            assert rows[0]['metadata_git_calls'] == rows[1]['metadata_git_calls']
        assert equal
        return {'scenario': case.name, 'equal': equal, 'rows': rows}
    finally:
        for receiver in receivers:
            receiver.release.set()
        for process in processes:
            try:
                if process.returncode is None and process.pid not in requested_stops:
                    await process.stdin.send(b'\n')
                    await process.stdin.aclose()
                with anyio.fail_after(5):
                    await process.wait()
            except (anyio.BrokenResourceError, anyio.ClosedResourceError, BrokenPipeError, TimeoutError) as error:
                errors.append(str(error))
                if process.returncode is None:
                    try:
                        process.kill()
                        with anyio.fail_after(3):
                            await process.wait()
                    except (ProcessLookupError, TimeoutError) as cleanup_error:
                        errors.append(str(cleanup_error))
            finally:
                try:
                    await process.aclose()
                except (anyio.BrokenResourceError, anyio.ClosedResourceError, OSError) as cleanup_error:
                    errors.append(str(cleanup_error))
        for log in logs:
            try:
                log.close()
            except OSError as cleanup_error:
                errors.append(str(cleanup_error))
        for receiver in receivers:
            try:
                await anyio.to_thread.run_sync(receiver.close)
            except OSError as cleanup_error:
                errors.append(str(cleanup_error))
        save(output/'observations.json', rows)
        save(output/'cleanup.json', {'exit_codes': [process.returncode for process in processes], 'errors': errors})
        assert not errors and all(process.returncode == 0 for process in processes)


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--composed', action='store_true')
    parser.add_argument('--held-stop', action='store_true')
    parser.add_argument('--non-utf8', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True)
    repo = output/'owned-repository'
    repository(repo)
    cases = [Case('automatic-session'), Case('static-token', token='  owned-token  '),
             Case('unhealthy-empty-token', health_status=503, expected_status=502), Case('unhealthy-static-token', health_status=401, token='owned-token', expected_status=502),
             Case('session-http-error', session='error', expected_status=500), Case('session-missing-token', session='missing-token', expected_status=500),
             Case('session-non-json', session='non-json', expected_status=500), Case('environment-target', environment_url=True),
             Case('invalid-target', invalid_url=True, expected_status=500), Case('get-405', method='GET', expected_status=405), Case('head-405', method='HEAD', expected_status=405)]
    if args.composed:
        cases = [cases[0], cases[2], cases[8]]
    if args.held_stop:
        cases = [Case('held-health-app-stop', token='owned-token', held='health'), Case('held-session-app-stop', held='session')]
        args.composed = True
    if args.non_utf8:
        cases = [Case('unrelated-non-utf8', unrelated_non_utf8=True)]
    invocation = {'command': [sys.executable, '-P', *sys.argv], 'binary': str(args.binary.resolve()) if args.binary else None,
                  'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary else None,
                  'normalization': 'elapsed_ms and owned receiver base URL only; error/status/session/metadata/request bodies unmasked'}
    save(output/'invocation.json', invocation)
    results = [await run_case(case, args.binary.resolve() if args.binary else None, output/case.name, repo, args.composed) for case in cases]
    save(output/'result.json', {'passed': len(results), 'diffs': 0, 'source_only': args.binary is None, 'scenarios': results})
    print(json.dumps({'passed': len(results), 'diffs': 0, 'output': str(output)}))


if __name__ == '__main__':
    anyio.run(main)

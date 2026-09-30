# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# Run with the original native msgq binding first on PYTHONPATH:
# PYTHONPATH=<msgq-python>:.:rust/tools python rust/tools/check_dmonitoring_daemon.py --help
from __future__ import annotations

import argparse
import hashlib
import json
import os
import select
import shutil
import signal
import subprocess
import time
from pathlib import Path

from check_monitoring_reference import compare
from dmonitoring_daemon_reference import Oracle, TOPICS, events, fixtures
from openpilot.cereal import log, messaging
from openpilot.cereal.services import SERVICE_LIST


def write_params(directory: Path, values) -> None:
    for key, value in values.items():
        (directory / key).write_bytes(b'1' if value else b'0')


def fields(value) -> int:
    return sum(fields(item) for item in value.values()) if isinstance(value, dict) else 1


def version(path: Path) -> tuple[int, int]:
    stat = path.stat()
    return stat.st_ino, stat.st_mtime_ns


def ready(process: subprocess.Popen, destination: Path) -> None:
    deadline = time.monotonic() + 5
    captured = b''
    while b'dmonitoringd: ready' not in captured:
        remaining = deadline - time.monotonic()
        assert remaining > 0 and select.select([process.stderr], [], [], remaining)[0], f'ready timeout: {destination}'
        chunk = os.read(process.stderr.fileno(), 65536)
        assert chunk, f'process exited before ready: {destination}'
        captured += chunk
        destination.write_bytes(captured)


def stop(process: subprocess.Popen) -> None:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def check(args, scenario: str):
    directory = args.output / scenario
    directory.mkdir()
    prefix = f'monitoring-qa-{os.getpid()}-{scenario}'
    shm = Path('/dev/shm') / f'msgq_{prefix}'
    shm.mkdir()
    params = directory / 'params' / prefix
    params.mkdir(parents=True)
    initial = {'IsRhdDetected': scenario != 'gating', 'AlwaysOnDM': scenario == 'saved',
               'IsDriverViewEnabled': False, 'DriverTooDistracted': scenario == 'saved'}
    write_params(params, initial)
    distracted_version, rhd_version = (version(params / key) for key in ('DriverTooDistracted', 'IsRhdDetected'))
    oracle = Oracle(initial)
    os.environ.update(OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(params.parent), SIMULATION='1')
    messaging.reset_context()
    publishers = {name: messaging.pub_sock(name) for name in TOPICS}
    subscriber = messaging.sub_sock('driverMonitoringState', timeout=5000)
    requests = list(fixtures(scenario))
    processes = []
    traces = []
    compared = 0
    expected_error = None
    try:
        process = subprocess.Popen([args.binary, '--frames', str(len(requests))], stderr=subprocess.PIPE, bufsize=0)
        processes.append(process)
        ready(process, directory / 'daemon.log')
        assert subscriber.receive(non_blocking=True) is None
        auxiliary = [value.to_bytes() for name, value in events(0, 0, scenario).items() if name != 'driverStateV2']
        (directory / 'auxiliary.bin').write_bytes(b''.join(auxiliary))
        for payload in auxiliary:
            with log.Event.from_bytes(payload) as event:
                publishers[event.which()].send(payload)
        assert oracle.step(auxiliary) is None
        subscriber.setTimeout(250)
        assert subscriber.receive() is None, 'auxiliary-only updates must not publish'
        subscriber.setTimeout(5000)
        with (directory / 'inputs.bin').open('wb') as inputs, (directory / 'expected.bin').open('wb') as expected_file, \
             (directory / 'actual.bin').open('wb') as actual_file, (directory / 'trace.jsonl').open('w') as trace_file:
            for index, (frame_id, changed, messages) in enumerate(requests):
                assert version(params / 'DriverTooDistracted') == distracted_version
                write_params(params, changed)
                oracle.params.values.update(changed)
                if 'DriverTooDistracted' in changed:
                    distracted_version = version(params / 'DriverTooDistracted')
                payloads = [messages[name].to_bytes() for name in (*TOPICS[1:], TOPICS[0])]
                inputs.write(b''.join(payloads))
                if scenario == 'gating' and frame_id == 6000:
                    assert (params / 'IsRhdDetected').read_bytes() == b'0'
                    assert version(params / 'IsRhdDetected') == rhd_version
                    assert oracle.snapshot()['wheel_samples'] > 300 and oracle.snapshot()['wheel_on_right']
                start = time.monotonic_ns()
                for name, payload in zip((*TOPICS[1:], TOPICS[0]), payloads, strict=True):
                    publishers[name].send(payload)
                if scenario == 'malformed' and index == 1:
                    try:
                        oracle.step(payloads)
                    except IndexError as error:
                        expected_error = str(error)
                    else:
                        raise AssertionError('original selected truncated orientation must fail')
                    assert process.wait(timeout=5) == 1
                    subscriber.setTimeout(200)
                    assert subscriber.receive() is None
                    break
                expected = oracle.step(payloads)
                assert expected is not None
                packet = subscriber.receive()
                assert packet is not None, f'missing output for frame {frame_id}'
                end = time.monotonic_ns()
                actual_file.write(packet)
                with log.Event.from_bytes(packet) as event:
                    actual = event.to_dict()
                assert start <= actual['logMonoTime'] <= end
                expected.logMonoTime = actual['logMonoTime']
                expected_file.write(expected.to_bytes())
                compare(expected.to_dict(), actual, 0., f'frame[{frame_id}]')
                compared += fields(actual)
                trace = dict(oracle.snapshot(), frame_id=frame_id, valid=actual['valid'],
                             published_always_on=actual['driverMonitoringState']['alwaysOn'], changed=changed)
                traces.append(trace)
                trace_file.write(json.dumps(trace) + '\n')
                assert subscriber.receive(non_blocking=True) is None, 'one driver update must publish once'
        if scenario != 'malformed':
            assert process.wait(timeout=5) == 0
        for name, value in oracle.params.values.items():
            assert (params / name).read_bytes() == (b'1' if value else b'0'), name
        assert version(params / 'DriverTooDistracted') == distracted_version
        if not oracle.params.writes:
            assert version(params / 'IsRhdDetected') == rhd_version
        if scenario == 'gating':
            assert oracle.params.writes == [('IsRhdDetected', True)]
            assert traces[1]['published_always_on'] is False and traces[2]['published_always_on'] is True
            assert traces[5]['published_always_on'] is True and traces[6]['published_always_on'] is False
            assert not traces[2]['valid'] and traces[2]['wheel_samples'] > traces[1]['wheel_samples']
            assert traces[3]['wheel_samples'] == traces[2]['wheel_samples']
            assert traces[6]['wheel_samples'] == traces[5]['wheel_samples']
        if scenario == 'saved':
            assert all(trace['lockout'] for trace in traces) and traces[1]['wheel_on_right']
        if scenario != 'malformed':
            for sig in (signal.SIGINT, signal.SIGTERM):
                waiting = subprocess.Popen([args.binary], stderr=subprocess.PIPE, bufsize=0)
                processes.append(waiting)
                ready(waiting, directory / f'wait-{sig.name}.log')
                subscriber.setTimeout(200)
                assert subscriber.receive() is None
                waiting.send_signal(sig)
                assert waiting.wait(timeout=5) == 0
        report = {'scenario': scenario, 'driver_updates': len(requests), 'publications': len(traces), 'compared_fields': compared,
                  'packet_float_tolerance': 0., 'discrete_fields': 'exact', 'parameter_writes': oracle.params.writes,
                  'driver_too_distracted_untouched': True, 'auxiliary_only_publications': 0,
                  'signal_checks': ['SIGINT no-driver wait', 'SIGTERM no-driver wait'] if scenario != 'malformed' else [],
                  'queue_capacities': {name: int(SERVICE_LIST[name].queue_size) for name in (*TOPICS, 'driverMonitoringState')},
                  'device_validation': False, 'final_original_state': oracle.snapshot(), 'original_error': expected_error}
        (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report), flush=True)
        return report
    finally:
        for index, process in reversed(list(enumerate(processes))):
            stop(process)
            (directory / f'process-{index}-tail.log').write_bytes(process.stderr.read())
        shutil.rmtree(shm)


def main() -> None:
    parser = argparse.ArgumentParser(description='Compare the continuous Rust monitoring daemon with the actual original loop and native IPC')
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.binary, args.output = args.binary.resolve(), args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    reports = [check(args, scenario) for scenario in ('gating', 'saved', 'malformed')]
    result = {'runs': reports, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'device_validation': False}
    (args.output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

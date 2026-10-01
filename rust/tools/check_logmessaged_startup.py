#!/usr/bin/env python3
import argparse
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import time

from logmessaged_native import Peer
from openpilot.cereal import log


def check_case(output, collector, shim, original, synchronize):
    output.mkdir(parents=True)
    gate = output / 'publisher.gate'
    gate.touch()
    original_popen = subprocess.Popen

    def delayed_publisher(*values, **keywords):
        keywords['env'] = dict(keywords['env'], LD_PRELOAD=str(shim), LOG_QA_PUBLISHER_GATE=str(gate))
        return original_popen(*values, **keywords)

    peer = Peer(collector, output / 'collector', original)
    try:
        if original:
            subprocess.Popen = delayed_publisher
            try:
                peer.start()
            finally:
                subprocess.Popen = original_popen
        else:
            peer.start()
        gate.unlink()
        if synchronize:
            peer.synchronize()
        marker = json.dumps({'msg': 'first-error-after-start', 'levelnum': 40})
        peer.socket.send_multipart([bytes([40]), marker.encode()])
        time.sleep(.2)
        observed = {name: [] for name in peer.subscribers}
        deadline = time.monotonic() + 1
        while time.monotonic() < deadline:
            for name, subscriber in peer.subscribers.items():
                packet = subscriber.receive(non_blocking=True)
                if packet is not None:
                    with (output / (name + '.bin')).open('ab') as stream:
                        stream.write(packet)
                    with log.Event.from_bytes(packet) as event:
                        assert event.valid and event.which() == name
                        observed[name].append(getattr(event, name))
            time.sleep(.005)
        (output / 'observed.json').write_text(json.dumps(observed, indent=2))
        expected = {'logMessage': [marker], 'errorLogMessage': [marker] if synchronize else []}
        assert observed == expected, observed
        disk = [json.loads(line) for path in peer.root.glob('swaglog.*') for line in path.read_text().splitlines()]
        assert len(disk) == 1 and disk[0]['msg$s'] == 'first-error-after-start', disk
        return {name: len(records) for name, records in observed.items()}
    finally:
        gate.unlink(missing_ok=True)
        peer.stop()
        peer.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--collector', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True)
    fixture = Path(__file__).with_name('fixtures') / 'logmessaged_publisher_pause.c'
    shim = output / 'publisher_pause.so'
    command = ['cc', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror', str(fixture), '-ldl', '-o', str(shim)]
    run = subprocess.run(command, capture_output=True, check=True)
    (output / 'compiler.stdout').write_bytes(run.stdout)
    (output / 'compiler.stderr').write_bytes(run.stderr)
    rows = {}
    for name, original, synchronize in [('legacy-source', True, False), ('ready-source', True, True), ('ready-native', False, True)]:
        rows[name] = check_case(output / name, args.collector.resolve(), shim, original, synchronize)
    report = {'passed': True, 'cases': rows, 'compiler': command, 'fixture_sha256': sha256(fixture.read_bytes()).hexdigest()}
    (output / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()

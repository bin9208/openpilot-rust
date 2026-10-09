#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Caller provides the existing original-source/full-cereal dependencies.
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import sys

from openpilot.cereal import log
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_report_source_controls import Fields, event
from carrot_server_dashcam_upload import save


class StateFields(Fields, total=False):
    yawRate: float
    steeringAngleDeg: float
    gasPressed: bool


def mono(seconds: float) -> int:
    return 1_000_000_000 + round(seconds * 1e9)


def car(seconds: float, acceleration: float, *, speed: float = 10.0, yaw: float = 0.0,
        angle: float = 0.0, steering: bool = False, brake: bool = False,
        gas: bool = False, standstill: bool = False, cancel: bool = False) -> bytes:
    data: StateFields = {'vEgo': speed, 'aEgo': acceleration, 'gearShifter': 'drive',
        'yawRate': yaw, 'steeringAngleDeg': angle, 'steeringPressed': steering,
        'brakePressed': brake, 'gasPressed': gas, 'standstill': standstill}
    if not cancel:
        return event('carState', mono(seconds), data)
    message = log.Event.new_message(); message.logMonoTime = mono(seconds)
    state = message.init('carState')
    for key, value in data.items(): setattr(state, key, value)
    button = state.init('buttonEvents', 1)[0]; button.type = 'cancel'; button.pressed = True
    return message.to_bytes()


def warning(seconds: float, names: list[str]) -> bytes:
    message = log.Event.new_message(); message.logMonoTime = mono(seconds)
    events = message.init('onroadEvents', len(names))
    for value, name in zip(events, names, strict=True): value.name = name
    return message.to_bytes()


def drive(seconds: float, enabled: bool, active: bool) -> bytes:
    return event('selfdriveState', mono(seconds), {'enabled': enabled, 'active': active})


def anchor(seconds: float = 0.0) -> bytes:
    return event('initData', mono(seconds), {'wallTimeNanos': 1_700_000_000_000_000_000 + round(seconds * 1e9)})


def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
    output = parser.parse_args().output.resolve(); output.mkdir(parents=True)
    catalog, paths, _ = source_modules()
    from openpilot.selfdrive.carrot.server.features.dashcam import report
    route = '2026-01-02--03-04-05'
    state = anchor() + drive(0, True, True) + car(0, 1.5, yaw=.4, steering=True)
    state += warning(.1, ['fcw', 'ldw', 'tooDistracted']) + warning(.2, ['driverDistracted1'])
    state += car(.25, 2.5, yaw=.4, steering=True, cancel=True) + drive(.5, False, False)
    state += car(.5, 0, speed=0, standstill=True) + drive(.75, True, True)
    state += car(.75, 1.5, angle=90, steering=True) + warning(.8, ['tooDistracted'])
    state += car(1, 0) + car(2, -3, brake=True) + drive(2.1, False, False)
    state += drive(3, True, True) + car(3, 0, speed=0, standstill=True)
    second = anchor(5)
    for seconds in [5.0, 5.25, 5.5, 5.75, 6.0]:
        second += car(seconds, 0, speed=0, standstill=True)
    second += drive(6.1, False, False)
    gap = anchor() + car(0, 1.5) + car(.25, 0) + car(1.5, 1.5) + car(1.75, 0)
    gap += car(2.99, 1.5) + car(3.1, 2.5) + car(3.25, 0)
    cap = anchor()
    for index in range(65): cap += car(index * 2.0, 1.5) + car(index * 2.0 + .25, 0)
    cases = [('state-carry', [state, second]), ('excursion-gap', [gap]), ('excursion-cap', [cap])]
    for name, segments in cases:
        root = output/name; root.mkdir()
        inputs = []
        for index, data in enumerate(segments):
            directory = root/(route + '--' + str(index)); directory.mkdir()
            path = directory/'rlog'; path.write_bytes(data)
            inputs.append({'file': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
        catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = report.DASHCAM_ROOT = str(root)
        result = report.build_route_report(route)
        save(root/'result.json', {'inputs': inputs, 'report': result})
        if name == 'state-carry':
            assert result['extras']['warnCounts'] == {'fcw': 1, 'ldw': 1, 'driverDistracted': 2}
            assert result['extras']['disengageCauses'] == {'button': 1, 'brake': 1, 'other': 1}
            assert result['extras']['cornerCount'] == 2
            assert result['time']['autoEnabledHms'] == '00:00:03'
            assert result['time']['autoActiveHms'] == '00:00:02'
        if name == 'excursion-gap':
            assert result['events']['hardAccel']['count'] == result['events']['overAccel']['count'] == 1
        if name == 'excursion-cap':
            assert result['events']['overAccel']['count'] == 65 and len(result['events']['overAccel']['items']) == 60
    save(output/'receipt.json', {'command': [sys.executable, '-P', *sys.argv], 'cases': len(cases),
        'source_sha256': hashlib.sha256(Path(report.__file__).read_bytes()).hexdigest(),
        'scope': 'only missing report state/warning/corner/cancel/active-reset and excursion gap/cap controls'})
    print('report state source controls PASS')


if __name__ == '__main__':
    main()

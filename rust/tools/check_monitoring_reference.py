# /// script
# dependencies = ["numpy==2.4.6", "pycapnp==2.1.0"]
# ///
"""Run: PYTHONPATH=.:rust/tools python rust/tools/check_monitoring_reference.py --help.

Execute the actual original policy, scalar statistics and filter class ASTs.
Only unavailable OS/IPC adapters are supplied; no policy math is duplicated.
"""
from __future__ import annotations

import argparse
import ast
from collections import Counter, defaultdict
from hashlib import sha256
import json
from math import atan2, radians
from pathlib import Path
import subprocess
from types import SimpleNamespace

import numpy as np
from openpilot.cereal import car, log
from monitoring_fixtures import fixtures

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ['openpilot/selfdrive/monitoring/policy.py', 'openpilot/common/stat_live.py',
           'openpilot/common/filter_simple.py', 'openpilot/common/realtime.py',
           'openpilot/common/transformations/camera.py', 'openpilot/cereal/log.capnp']


class Parameters:
    initial = False

    def get_bool(self, name):
        assert name == 'DriverTooDistracted'
        return self.initial


def message(service, valid=True):
    event = log.Event.new_message()
    event.valid = valid
    event.init(service)
    return event


def original_class():
    rt = ast.parse((ROOT / SOURCES[3]).read_text())
    dt = next(ast.literal_eval(node.value) for node in rt.body if isinstance(node, ast.Assign)
              and any(isinstance(t, ast.Name) and t.id == 'DT_DMON' for t in node.targets))
    camera = ast.parse((ROOT / SOURCES[4]).read_text())
    dimensions = next(node.value.args[:2] for node in camera.body if isinstance(node, ast.Assign)
                      and any(isinstance(t, ast.Name) and t.id == '_ar_ox_fisheye' for t in node.targets))
    policy_tree = ast.parse((ROOT / SOURCES[0]).read_text())
    focal = next(ast.literal_eval(node.value) for node in policy_tree.body if isinstance(node, ast.Assign)
                 and any(isinstance(t, ast.Name) and t.id == 'dcam_undistorted_FL' for t in node.targets))
    namespace = {'np': np, 'defaultdict': defaultdict, 'atan2': atan2, 'radians': radians, 'Params': Parameters,
                     'messaging': SimpleNamespace(new_message=message), 'car': car, 'log': log, 'DT_DMON': dt,
                     'dcam_undistorted_W': ast.literal_eval(dimensions[0]), 'dcam_undistorted_H': ast.literal_eval(dimensions[1]),
                     'dcam_undistorted_FL': focal, 'AlertLevel': log.DriverMonitoringState.AlertLevel,
                     'MonitoringPolicy': log.DriverMonitoringState.MonitoringPolicy}
    for path, names in [(SOURCES[1], {'RunningStat', 'RunningStatFilter'}),
                        (SOURCES[2], {'FirstOrderFilter'}), (SOURCES[0], None)]:
        tree = ast.parse((ROOT / path).read_text())
        tree.body = [node for node in tree.body if isinstance(node, (ast.ClassDef, ast.FunctionDef))
                     and (names is None or node.name in names)]
        exec(compile(tree, str(ROOT / path), 'exec'), namespace)
    return namespace['DriverMonitoring']


def camel(name):
    first, *rest = name.split('_')
    return first + ''.join(word.capitalize() for word in rest)


def original_input(value):
    ds = value['driver']
    def driver(side):
        return SimpleNamespace(**{camel(key): [] if item is None else item for key, item in side.items()})
    return {'carState': SimpleNamespace(vEgo=value['car_speed'], steeringPressed=value['steering_pressed'],
                gasPressed=value['gas_pressed'], steeringAngleDeg=value['steering_angle_deg'],
                gearShifter=car.CarState.GearShifter.park if value['wrong_gear'] else car.CarState.GearShifter.drive),
            'selfdriveState': SimpleNamespace(enabled=value['enabled']),
            'driverStateV2': SimpleNamespace(leftDriverData=driver(ds['left']), rightDriverData=driver(ds['right']),
                                               wheelOnRightProb=ds['wheel_on_right_prob']),
            'liveCalibration': SimpleNamespace(rpyCalib=value['calibration']),
            'modelV2': SimpleNamespace(meta=SimpleNamespace(disengagePredictions=SimpleNamespace(brakeDisengageProbs=[value['brake_disengage_prob']]))) }


def primitive(value):
    if isinstance(value, (bool, str, int, float)) or value is None:
        return value
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, dict):
        return {key: primitive(item) for key, item in value.items()}
    return {key: primitive(item) for key, item in vars(value).items() if key not in ('M_last', 'S_last')}


def state(dm):
    output = {key: primitive(item) for key, item in vars(dm).items()
              if key not in ('settings', 'alert_level', 'active_policy', 'no_response_timeout', 'driver_distraction_filter')}
    output['alert_level'] = ['None', 'One', 'Two', 'Three'][int(dm.alert_level)]
    output['active_policy'] = ['Wheeltouch', 'Vision'][int(dm.active_policy)]
    output['driver_distraction_filter'] = float(dm.driver_distraction_filter.x)
    for stat in [output['wheelpos_offsetter'], output['pose']['pitch_offsetter'], output['pose']['yaw_offsetter']]:
        for value in stat.values():
            if value['max_trackable'] == -1:
                value['max_trackable'] = None
    return output


def compare(expected, actual, tolerance, path=''):
    if isinstance(expected, dict):
        assert expected.keys() == actual.keys(), (path, expected.keys(), actual.keys())
        return max((compare(item, actual[key], tolerance, f'{path}.{key}') for key, item in expected.items()), default=0.)
    if isinstance(expected, float):
        error = abs(expected - actual)
        assert error <= tolerance, (path, expected, actual, error)
        return error
    assert type(expected) is type(actual) and expected == actual, (path, expected, actual)
    return 0.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    policy = original_class()
    coverage = Counter()
    requests = args.output / 'request.jsonl'
    references = args.output / 'reference.jsonl'
    expected_packets = args.output / 'reference.bin'
    with requests.open('w') as req, references.open('w') as ref, expected_packets.open('wb') as packets:
        for index, frame in enumerate(fixtures()):
            if frame['reset']:
                Parameters.initial = frame['too_distracted']
                dm = policy(rhd_saved=frame['rhd_saved'], always_on=frame['always_on'])
            dm.run_step(original_input(frame['input']), demo=frame['input']['demo'])
            event = dm.get_state_packet(valid=frame['valid'])
            event.logMonoTime = index
            req.write(json.dumps(frame) + '\n')
            ref.write(json.dumps(state(dm)) + '\n')
            packets.write(event.to_bytes())
            coverage[frame['scenario']] += 1
            coverage[f'alert:{int(dm.alert_level)}'] += 1
            coverage[f'policy:{int(dm.active_policy)}'] += 1
            coverage[f'calibrated:{dm.pose.calibrated}'] += 1
            coverage[f'lockout:{dm.too_distracted}'] += 1
    actual_state = args.output / 'actual.jsonl'
    actual_packets = args.output / 'actual.bin'
    subprocess.run([args.binary.resolve(), requests, actual_state, actual_packets], check=True)
    maximum = 0.
    count = 0
    with references.open() as expected, actual_state.open() as actual:
        for index, (a, b) in enumerate(zip(expected, actual, strict=True)):
            maximum = max(maximum, compare(json.loads(a), json.loads(b), 1e-10, f'frame[{index}]'))
            count += 1
    packets = 0
    with expected_packets.open('rb') as expected, actual_packets.open('rb') as actual:
        for index, (a, b) in enumerate(zip(log.Event.read_multiple(expected), log.Event.read_multiple(actual), strict=True)):
            compare(a.to_dict(), b.to_dict(), 0., f'packet[{index}]')
            packets += 1
    assert packets == count
    assert all(coverage[f'alert:{level}'] for level in range(4))
    assert all(coverage[f'policy:{mode}'] for mode in range(2))
    assert coverage['calibrated:True'] and coverage['lockout:True']
    report = {'frames': count, 'packets': packets, 'state_tolerance': 1e-10, 'maximum_absolute_error': maximum,
                  'packet_float_tolerance': 0., 'coverage': coverage, 'device_validation': False,
                  'source_sha256': {path: sha256((ROOT / path).read_bytes()).hexdigest() for path in SOURCES}}
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()

"""Actual monitoring loop statements and full cereal fixtures for native daemon QA."""
from __future__ import annotations

import ast
from pathlib import Path
from types import SimpleNamespace

from check_message_state import source
from check_monitoring_reference import Parameters, camel, message, original_class
from monitoring_fixtures import driver
from openpilot.cereal import log

TOPICS = ('driverStateV2', 'liveCalibration', 'carState', 'selfdriveState', 'modelV2')


class LoopParams:
    """In-memory original Params adapter recording only requested loop writes."""
    def __init__(self, values):
        self.values = dict(values)
        self.writes = []

    def get_bool(self, name):
        return self.values.get(name, False)

    def put_bool(self, name, value):
        self.values[name] = value
        self.writes.append((name, value))


class Oracle:
    """Mutable original policy and original SubMaster state, without native I/O."""
    def __init__(self, values):
        Parameters.initial = values['DriverTooDistracted']
        self.params = LoopParams(values)
        messaging, environment = source()
        environment['simulation'] = '1'
        self.sm = messaging['SubMaster'](list(TOPICS), poll='driverStateV2')
        self.packet = None
        self.scope = {'params': self.params, 'sm': self.sm, 'DriverMonitoring': original_class(),
                      'pm': SimpleNamespace(send=self.publish)}
        path = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/monitoring/dmonitoringd.py'
        module = ast.parse(path.read_text())
        function = next(node for node in module.body if isinstance(node, ast.FunctionDef) and node.name == 'dmonitoringd_thread')
        initial = [node for node in function.body if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) in ('DM', 'demo_mode')]
        exec(compile(ast.Module(body=initial, type_ignores=[]), str(path), 'exec'), self.scope)
        loop = next(node for node in function.body if isinstance(node, ast.While))
        one_iteration = ast.For(target=ast.Name(id='_once', ctx=ast.Store()), iter=ast.Tuple(elts=[ast.Constant(None)], ctx=ast.Load()),
                                body=loop.body[1:], orelse=[])
        self.step_code = compile(ast.fix_missing_locations(ast.Module(body=[one_iteration], type_ignores=[])), str(path), 'exec')

    def publish(self, topic, packet):
        assert topic == 'driverMonitoringState' and self.packet is None
        self.packet = packet

    def step(self, payloads):
        self.packet = None
        decoded = list(log.Event.read_multiple_bytes(b''.join(payloads)))
        self.sm.update_msgs(1. + (self.sm.frame + 2) * .05, decoded)
        exec(self.step_code, self.scope)
        return self.packet

    def snapshot(self):
        dm = self.scope['DM']
        return {'always_on': dm.always_on, 'demo': self.scope['demo_mode'], 'wheel_on_right': dm.wheel_on_right,
                'wheel_samples': dm.wheelpos_offsetter.filtered_stat.n, 'wheel_mean': dm.wheelpos_offsetter.filtered_stat.M,
                'lockout': dm.too_distracted, 'writes': list(self.params.writes)}


def events(frame_id: int, index: int, scenario: str):
    result = {topic: message(topic) for topic in TOPICS}
    result['carState'].carState.from_dict({'vEgo': 20., 'gearShifter': 'drive', 'steeringAngleDeg': 33.125})
    result['selfdriveState'].selfdriveState.enabled = False
    result['liveCalibration'].liveCalibration.rpyCalib = [0., .01234567, -.02345678]
    result['modelV2'].modelV2.meta.disengagePredictions.brakeDisengageProbs = [.81234567]
    left, right = driver(), driver()
    for side in (left, right):
        side['face_orientation'] = [.02345678, -.03456789, 0.]
        side['face_position'] = [.01234567, -.001234567]
    if scenario == 'gating':
        if index in (2, 3, 4, 5, 6):
            result['carState'].valid = False
        result['driverStateV2'].valid = index != 3 and frame_id != 6000
        if index <= 6:
            left['phone_prob'] = right['phone_prob'] = 1.
        if 7 <= index <= 10:
            field = ('face_orientation', 'face_position', 'face_orientation_std', 'face_position_std')[index - 7]
            left[field] = right[field] = []
    if scenario == 'saved':
        result['driverStateV2'].valid = index != 0
        left['face_orientation'] = [0.]
        if index == 2:
            right['face_orientation'] += [.123]
            right['face_position'] += [.123]
            right['face_orientation_std'] += [.123]
            right['face_position_std'] = [.123]
        if index == 3:
            right['face_orientation'] = []
            right['face_position'] = [0.]
            right['face_orientation_std'] = [0.]
    if scenario == 'malformed':
        left['face_orientation'] = [0.]
        if index == 1:
            right['face_orientation'] = [0.]
    data = result['driverStateV2'].driverStateV2
    data.frameId = frame_id
    data.wheelOnRightProb = 1.
    data.leftDriverData.from_dict({camel(key): value for key, value in left.items()})
    data.rightDriverData.from_dict({camel(key): value for key, value in right.items()})
    return result


def fixtures(scenario: str):
    if scenario == 'gating':
        ids = [0, 1, 2, 3, 40, 41, 42, 43, 44, 45, 46, *range(100, 520), 6000]
    elif scenario == 'saved':
        ids = [0, 7, 8, 9]
    else:
        assert scenario == 'malformed'
        ids = [7, 8]
    for index, frame_id in enumerate(ids):
        changed = {}
        if scenario == 'gating' and index in (1, 4):
            changed = {'AlwaysOnDM': index == 1, 'IsDriverViewEnabled': index == 1}
        if scenario == 'saved' and index == 1:
            changed = {'DriverTooDistracted': False}
        yield frame_id, changed, events(frame_id, index, scenario)

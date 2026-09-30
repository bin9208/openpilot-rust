from __future__ import annotations

import argparse
import ast
from collections import Counter, deque
import copy
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType, SimpleNamespace

import numpy as np


class Parameters:
    def __init__(self):
        self.values = {"LaneChangeNeedTorque": 0, "LaneChangeBsd": 0, "LaneLineCheck": 0, "LaneChangeDelay": 0}
        self.reads = []

    def get_int(self, name):
        self.reads.append(name)
        return int(self.values[name])

    def get_float(self, name):
        self.reads.append(name)
        return float(self.values[name])


class Commands:
    def __init__(self, action):
        self.action = action
        self.allowed = []

    def read(self, allowed):
        self.allowed.append(allowed)
        return self.action if allowed else None


def original_class():
    root = Path(__file__).resolve().parents[2]
    tree = ast.parse((root / "openpilot/common/realtime.py").read_text())
    timestep = next(ast.literal_eval(node.value) for node in tree.body if isinstance(node, ast.Assign)
                    and any(isinstance(target, ast.Name) and target.id == "DT_MDL" for target in node.targets))
    realtime = ModuleType("openpilot.common.realtime")
    realtime.DT_MDL = timestep
    params = ModuleType("openpilot.common.params")
    params.Params = Parameters
    sys.modules[realtime.__name__] = realtime
    sys.modules[params.__name__] = params
    from openpilot.selfdrive.controls.lib.desire_helper import DesireHelper
    return DesireHelper


def input_template():
    return {
        "car": {"can_valid": True, "left_blinker": False, "right_blinker": False, "v_ego": 20., "a_ego": 0.,
                "trailer_connected": False, "steering_torque": 0., "steering_pressed": False, "steering_angle_deg": 0.,
                "left_lane_line": 0, "right_lane_line": 0, "left_blindspot": False, "right_blindspot": False},
        "model": {"lane_lines": [[value] * 33 for value in [-5.4, -1.8, 1.8, 5.4]], "lane_line_probs": [1.] * 4,
                  "road_edges": [[value] * 33 for value in [-7.2, 7.2]], "desire_state": [0.] * 8, "orientation_rate_z": [0.] * 33},
        "navigation": {"atc_type": "", "command_index": 0, "command": "", "argument": ""},
        "leads": [{"status": False}, {"status": False}], "objects": [[], []], "lateral_active": True, "lane_change_prob": .1,
    }


def fixtures():
    random = np.random.default_rng(20260930)
    for sequence in range(40):
        config = {"need_torque": int(random.choice([-1, 0, 1])), "bsd": int(random.choice([-1, 0, 1])),
                  "line_check": int(random.integers(0, 3)), "delay_tenths": float(random.choice([0., 1., 5., 10.]))}
        value = input_template()
        for frame in range(600):
            if frame % 25 == 0:
                car = value["car"]
                car.update(v_ego=float(random.choice([0., 5., 30 / 3.6, 10., 20., 30.])), a_ego=float(random.uniform(-3, 2)),
                           left_blinker=bool(random.integers(0, 2)), right_blinker=bool(random.random() < .2),
                           steering_pressed=bool(random.random() < .4), steering_torque=float(random.choice([-1., 0., 1.])),
                           steering_angle_deg=float(random.choice([-90, 0, 90])), trailer_connected=bool(random.random() < .08),
                           left_blindspot=bool(random.random() < .15), right_blindspot=bool(random.random() < .15),
                           left_lane_line=int(random.choice([-15, 0, 5, 11, 20, 25, 30])), right_lane_line=int(random.choice([0, 5, 11, 20, 25, 30])),
                           can_valid=bool(random.random() > .05))
                value["lateral_active"] = bool(random.random() > .1)
                value["navigation"]["atc_type"] = str(random.choice(["", "turn left", "turn right", "fork left", "fork right", "atc left", "atc right"]))
                value["navigation"].update(command_index=frame, command="LANECHANGE" if random.random() < .15 else "",
                                           argument=str(random.choice(["LEFT", "RIGHT"])))
                value["lane_change_prob"] = float(random.choice([0., .01, .02, .1]))
                model = value["model"]
                model["lane_line_probs"] = random.choice([0., .3, .5, .8, 1.], size=4).tolist()
                model["desire_state"] = [0., float(random.choice([0., .1, .2])), 0., 0., 0., 0., 0., 0.]
                model["orientation_rate_z"] = np.linspace(float(random.uniform(-1, 1)), float(random.uniform(-1, 1)), 33).tolist()
                for index in range(4):
                    model["lane_lines"][index] = np.linspace([-5.4, -1.8, 1.8, 5.4][index] + float(random.uniform(-2, 2)),
                                                            [-5.4, -1.8, 1.8, 5.4][index] + float(random.uniform(-2, 2)), 33).tolist()
                model["road_edges"] = [np.linspace(sign * float(random.uniform(2, 9)), sign * float(random.uniform(2, 9)), 33).tolist() for sign in [-1, 1]]
                value["leads"] = [{"status": bool(random.random() < .3), "d_rel": float(random.uniform(.05, 170)),
                                   "v_rel": float(random.uniform(-15, 12)), "radar_track_id": 1} for _ in range(2)]
                value["objects"] = [[{"status": True, "d_rel": float(random.uniform(.05, 50)), "v_rel": float(random.uniform(-10, 15)),
                                      "radar_track_id": int(random.choice([46, 200, 219, 220, 240, 249, 250, 300, 411, 412, 1661, 1777]))}
                                     for _ in range(int(random.integers(0, 4)))] for _ in range(2)]
            remote = "laneLeft" if frame % 113 == 0 else "laneRight" if frame % 127 == 0 else None
            yield {"reset": frame == 0, "input": copy.deepcopy(value), "config": config, "remote": remote, "scenario": f"random-{sequence}"}
    for direction in ("left", "right"):
        value = input_template()
        config = {"need_torque": 0, "bsd": 0, "line_check": 0, "delay_tenths": 0.}
        for frame in range(180):
            value["car"][f"{direction}_blinker"] = 15 <= frame < 150
            value["lane_change_prob"] = 0.
            value["car"]["steering_pressed"] = frame >= 65
            value["car"]["steering_torque"] = 1. if direction == "left" else -1.
            yield {"reset": frame == 0, "input": copy.deepcopy(value), "config": config, "remote": None, "scenario": f"full-cycle-{direction}"}
    for scenario in ("receding", "changing-track", "unsafe-peer", "front-track", "far-track"):
        value = input_template()
        config = {"need_torque": 0, "bsd": 0, "line_check": 0, "delay_tenths": 0.}
        for frame in range(45):
            value["car"].update(v_ego=25., left_blindspot=frame == 0)
            if frame:
                identifier = 1700 + frame if scenario == "changing-track" else 46 if scenario == "front-track" else 1661
                distance = (40. if scenario == "far-track" else 4.) + (frame - 1) * .45
                value["objects"][0] = [{"status": True, "d_rel": distance, "v_rel": 9., "radar_track_id": identifier}]
                if scenario == "unsafe-peer":
                    value["objects"][0].append({"status": True, "d_rel": 8., "v_rel": -5., "radar_track_id": 1777})
            yield {"reset": frame == 0, "input": copy.deepcopy(value), "config": config, "remote": None, "scenario": scenario}


def camel(name):
    first, *rest = name.split("_")
    return first + "".join(part.capitalize() for part in rest)


def original_input(value):
    car = SimpleNamespace(**{camel(name): entry for name, entry in value["car"].items()})
    model = value["model"]
    model = SimpleNamespace(laneLines=[SimpleNamespace(y=entry) for entry in model["lane_lines"]], laneLineProbs=model["lane_line_probs"],
                            roadEdges=[SimpleNamespace(y=entry) for entry in model["road_edges"]], meta=SimpleNamespace(desireState=model["desire_state"]),
                            orientationRate=SimpleNamespace(z=model["orientation_rate_z"]))
    navigation = value["navigation"]
    navigation = SimpleNamespace(atcType=navigation["atc_type"], carrotCmdIndex=navigation["command_index"],
                                 carrotCmd=navigation["command"], carrotArg=navigation["argument"])
    leads = [SimpleNamespace(**{camel(name): entry for name, entry in lead.items() if entry is not None}) for lead in value["leads"]]
    objects = [[SimpleNamespace(**{camel(name): entry for name, entry in lead.items() if entry is not None}) for lead in side] for side in value["objects"]]
    radar = SimpleNamespace(leadLeft=leads[0], leadRight=leads[1], leadsLeft=objects[0], leadsRight=objects[1])
    return car, model, navigation, radar


def primitive(value):
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    if isinstance(value, (list, tuple, deque)):
        return [primitive(entry) for entry in value]
    if isinstance(value, np.generic):
        return value.item()
    return {key: primitive(entry) for key, entry in vars(value).items() if key != "threshold"}


def compare(expected, actual, path=""):
    if isinstance(expected, dict):
        assert expected.keys() == actual.keys(), (path, expected.keys(), actual.keys())
        return max((compare(value, actual[key], f"{path}.{key}") for key, value in expected.items()), default=0.)
    if isinstance(expected, list):
        assert len(expected) == len(actual), path
        return max((compare(a, b, f"{path}[{i}]") for i, (a, b) in enumerate(zip(expected, actual, strict=True))), default=0.)
    if isinstance(expected, float):
        error = abs(expected - actual)
        assert error <= 1e-10, (path, expected, actual)
        return error
    assert expected == actual, (path, expected, actual)
    return 0.


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    original = original_class()
    helper = None
    config = None
    counts = Counter()
    request_path = args.output / "request.jsonl"
    reference_path = args.output / "reference.jsonl"
    with request_path.open("w") as requests, reference_path.open("w") as references:
        for frame in fixtures():
            requests.write(json.dumps(frame) + "\n")
            if frame["reset"]:
                helper = original()
                config = {"need_torque": 0, "bsd": 0, "line_check": 0, "delay_tenths": 0.}
            settings = frame["config"]
            helper.params.values = dict(zip(["LaneChangeNeedTorque", "LaneChangeBsd", "LaneLineCheck", "LaneChangeDelay"], settings.values(), strict=True))
            helper.params.reads.clear()
            helper.bluetooth_commands = Commands(frame["remote"])
            car, model, navigation, radar = original_input(frame["input"])
            helper.update(car, model, frame["input"]["lateral_active"], frame["input"]["lane_change_prob"], navigation, radar)
            if helper.frame == 7 and frame["scenario"] in ("receding", "changing-track", "unsafe-peer", "front-track", "far-track"):
                assert (helper.left.bsd_hold_counter == 0) == (frame["scenario"] == "receding")
            if helper.params.reads:
                assert helper.params.reads == ["LaneChangeNeedTorque", "LaneChangeBsd", "LaneLineCheck", "LaneChangeDelay"]
                config = settings
            state = {key: primitive(value) for key, value in vars(helper).items() if key not in (
                "params", "bluetooth_commands", "lane_change_state", "lane_change_direction", "turn_direction", "desire", "maneuver_type",
                "laneChangeNeedTorque", "laneChangeBsd", "laneLineCheck", "laneChangeDelay", "desireLog")}
            state.update(lane_change_state=["Off", "PreLaneChange", "Starting", "Finishing"][int(helper.lane_change_state)],
                         lane_change_direction=int(helper.lane_change_direction), turn_direction=int(helper.turn_direction), desire=int(helper.desire),
                         maneuver_type={"none": "None", "turn": "Turn", "lane_change": "LaneChange"}[helper.maneuver_type],
                         config=config, desire_log=helper.desireLog)
            references.write(json.dumps({"state": state, "remote_allowed": helper.bluetooth_commands.allowed[0],
                                          "params_refreshed": bool(helper.params.reads)}) + "\n")
            counts[f"state:{state['lane_change_state']}"] += 1
            counts[f"desire:{state['desire']}"] += 1
    actual_path = args.output / "actual.jsonl"
    subprocess.run([args.binary.resolve(), request_path.resolve(), actual_path.resolve()], check=True)
    maximum_error = 0.
    frames = 0
    with reference_path.open() as expected, actual_path.open() as observed:
        for index, (a, b) in enumerate(zip(expected, observed, strict=True)):
            maximum_error = max(maximum_error, compare(json.loads(a), json.loads(b), str(index)))
            frames += 1
    report = {"frames": frames, "maximum_absolute_error": maximum_error, "numeric_tolerance": 1e-10,
              "state_coverage": counts, "device_validation": False}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()

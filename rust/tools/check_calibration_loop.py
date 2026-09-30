#!/usr/bin/env python3
"""Execute the original calibration main-loop body against Rust SubMaster/controller steps."""
from __future__ import annotations

import argparse
from collections import Counter
import json
from functools import partial
from pathlib import Path
import subprocess
from types import SimpleNamespace

import numpy as np

from calibration_reference import Parameters, compare_packet, original
from check_message_state import source as messaging_source
from openpilot.cereal import log


def frames(new_message):
    for simulation, not_car in ((False, False), (True, False), (True, True)):
        for index in range(1200):
            camera = new_message("cameraOdometry", valid=not 200 <= index < 220)
            camera.cameraOdometry.trans = [10., 0., 0.]
            camera.cameraOdometry.rot = [0., 0., 0.]
            camera.cameraOdometry.transStd = [0., 0., 0.]
            camera.cameraOdometry.wideFromDeviceEuler = [0.01, 0.02, 0.03]
            camera.cameraOdometry.roadTransformTrans = [0., 0., 1.5]
            camera.cameraOdometry.roadTransformTransStd = [0., 0., 0.]
            car = new_message("carState", valid=not 100 <= index < 120)
            car.carState.vEgo = 0. if index % 37 == 0 else 10.
            silent = index < 5 or 300 <= index < 350
            messages = [] if silent else [camera] if 350 <= index < 400 else [camera, car]
            trim = "1" if 600 <= index < 750 else "0.0001" if 800 <= index < 825 else "0.00010000000474974513" if 825 <= index < 850 else "0"
            yield {"configuration": {"simulation": simulation, "not_car": not_car, "mici": False, "saved": None} if index == 0 else None,
                   "time": 100. + index * .05, "timestamp": index + 1000,
                   "messages": [list(message.to_bytes()) for message in messages], "trim": trim}, messages


def check(binary: Path, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=False)
    scope, environment = messaging_source()
    counters = Counter()
    params = None
    with subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1) as process, \
         (output / "trace.jsonl").open("w") as trace:
        assert process.stdin is not None and process.stdout is not None
        for request, messages in frames(scope["new_message"]):
            if request["configuration"] is not None:
                config = request["configuration"]
                environment["simulation"] = "1" if config["simulation"] else "0"
                sm = scope["SubMaster"](["cameraOdometry", "carState"], poll="cameraOdometry")
                params = Parameters()
                source = original(False, params)
                calibrator = source.Calibrator(param_put=True)
                calibrator.not_car = config["not_car"]
                sent, timeouts = [], []
                context = {**source.__dict__, "calibrator": calibrator, "sm": sm, "params_reader": params,
                           "pm": SimpleNamespace(send=lambda topic, packet, sent=sent: sent.append(packet)), "DEBUG": False}
            assert params is not None
            params.trim = float(np.float32(request["trim"]))
            before_writes, before_sent = len(params.writes), len(sent)

            bound_update = partial(sm.update_msgs, request["time"], [message.as_reader() for message in messages])

            def update(timeout, callback=bound_update, recorded=timeouts):
                recorded.append(timeout)
                callback()

            sm.update = update
            exec(source.loop_body, context)
            process.stdin.write(json.dumps(request) + "\n")
            process.stdin.flush()
            actual = json.loads(process.stdout.readline())
            published, persisted = len(sent) != before_sent, len(params.writes) != before_writes
            try:
                assert actual["frame"] == sm.frame and actual["timeout"] == timeouts[-1]
                assert actual["publish"] == published and actual["valid"] == sm.all_checks()
                assert actual["accepted"] == (sm.updated["cameraOdometry"] and context.get("new_rpy") is not None)
                assert actual["idx"] == calibrator.idx and actual["block_idx"] == calibrator.block_idx
                assert (actual["persisted"] is not None) == persisted
                if published:
                    sent[-1].logMonoTime = request["timestamp"]
                    with log.Event.from_bytes(bytes(actual["packet"])) as packet:
                        counters["packet_fields"] += compare_packet(packet.to_dict(), sent[-1].to_dict())
                    counters["valid_publications" if actual["valid"] else "invalid_publications"] += 1
                if persisted:
                    with log.Event.from_bytes(params.writes[-1]) as saved:
                        expected = saved.to_dict()
                    expected["logMonoTime"] = request["timestamp"]
                    with log.Event.from_bytes(bytes(actual["persisted"])) as packet:
                        counters["persisted_fields"] += compare_packet(packet.to_dict(), expected)
                counters["publications"] += int(published)
                counters["persistence_steps"] += int(persisted)
                counters["frozen_updates"] += int(sm.updated["cameraOdometry"] and abs(params.trim * .01) > 1e-6
                                                   and calibrator.cal_status == log.LiveCalibrationData.Status.calibrated
                                                   and context.get("new_rpy") is None)
                counters["timeout_frames"] += int(not messages)
                counters["steps"] += 1
                trace.write(json.dumps(actual) + "\n")
            except AssertionError:
                (output / "failure.json").write_text(json.dumps({"request": request, "actual": actual,
                                                                "expected_valid": sm.all_checks(), "expected_publish": published}, indent=2) + "\n")
                raise
        process.stdin.close()
        assert process.wait(timeout=5) == 0
    assert counters["publications"] == 720 and counters["frozen_updates"] > 0 and counters["persistence_steps"] > 0
    assert counters["valid_publications"] and counters["invalid_publications"]
    (output / "report.json").write_text(json.dumps({"result": "pass", **counters}, indent=2) + "\n")
    print(json.dumps(counters, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    check(args.binary.resolve(), args.output.resolve())

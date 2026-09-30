# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy"]
# ///
# Run: PYTHONPATH=. uv run rust/tools/check_jetlink_reference.py <probe> <evidence>
"""Run the real original Jetlink policy/adapter and wire builders as independent oracles."""
from __future__ import annotations

import itertools
import json
import random
import subprocess
import sys
from pathlib import Path

import numpy as np

from openpilot.selfdrive.modeld.jetlink.client import CONTRACT
from openpilot.selfdrive.modeld.jetlink.model_state import JetlinkModelState
from openpilot.selfdrive.modeld.jetlink.owner import GadgetOwner
from openpilot.selfdrive.modeld.jetlink.transition import ControlState, Outcome, Transition
from openpilot.selfdrive.modeld.jetlink.validation import validation_matches
from openpilot.selfdrive.modeld.parse_model_outputs import Parser
from third_party.jetlink import protocol  # noqa: TID251 - pinned Jetlink is vendored at the repository root.
from third_party.jetlink.spec import ModelSpec  # noqa: TID251 - pinned Jetlink is vendored at the repository root.
from third_party.jetlink.transport.ffs import build_descriptors, build_strings  # noqa: TID251 - pinned Jetlink is vendored at the repository root.


def main() -> None:
    """Discrete decisions/bytes are exact; parsed float32 uses predeclared atol=rtol=1e-6."""
    binary, evidence = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    requests, expected, labels = [], [], []
    def add(request, answer, label):
        requests.append(request)
        expected.append(answer)
        labels.append(label)

    rng = random.Random(4400)
    choices = list(itertools.product(range(3), (False, True), (False, True), itertools.product((False, True), repeat=4), list(Outcome)))
    for restarted in (False, True):
        state, steps, answers = Transition(restarted), [], []
        for _ in range(20000):
            mode, ready, valid, controls, outcome = rng.choice(choices)
            controls = ControlState(*controls)
            steps.append({"mode": ["Off", "Shadow", "ActiveRequest"][mode], "ready": ready, "validated": valid,
                          "controls": vars(controls), "outcome": outcome.name.title()})
            answers.append(vars(state.update(mode, ready, valid, controls, outcome)))
        add({"op": "transition", "previously_active": restarted, "steps": steps}, answers, "transition")
    identity = {"device_model": "synthetic", "android_api": "35", "backend_requested": "cpu", "runtime_version": "1.22.0",
                "app_version": "oracle", "artifact_sha256": CONTRACT["sha256"]}
    record = {"approved": True, "device_test": True, "numerical_parity": True, "model_sha256": CONTRACT["sha256"], "identity": identity,
              "warp_contract": "native-c3x-512x256-v1", "duration_seconds": 1800, "end_to_end_max_ms": 50,
              "deadline_misses": 0, "validation_id": "a" * 64}
    variants = [(record, identity), ({}, identity)]
    for key in record:
        for value in (None, False, True, 0, 0.0, -1, 49.999, 50.001, 1799.999, 1800, "bad", [], {}):
            variants.append((record | {key: value}, identity))
    for key in identity:
        variants.append((record, identity | {key: "different"}))
        variants.append((record, {k: v for k, v in identity.items() if k != key}))
    for variant, ident in variants:
        raw = json.dumps(variant)
        add({"op": "validate", "raw": raw, "identity": ident}, validation_matches(raw, ident), "validation")
    for raw in (None, "{bad", " " * 16385):
        add({"op": "validate", "raw": raw, "identity": identity}, validation_matches(raw, identity), "validation")
    images = np.arange(2 * 6 * 128 * 256, dtype=np.uint8).reshape(2, 6, 128, 256)
    adapter = JetlinkModelState(1928, 1208, None, ModelSpec.from_dict(CONTRACT), lambda *_: images)
    steps, answers = [], []
    for index in range(500):
        desire = np.array([rng.choice((0, 1, .98, .99, 1.01)) for _ in range(8)], np.float32)
        traffic, action = np.array([1, 0], np.float32), np.array([.3, .8], np.float32)
        prepare_only = index % 17 == 0
        prepared = adapter.prepare({}, {}, {"desire_pulse": desire, "traffic_convention": traffic, "action_t": action}, prepare_only)
        steps.append({"desire": desire.tolist(), "traffic": traffic.tolist(), "action": action.tolist(), "prepare_only": prepare_only})
        answers.append({"packed": None if prepared is None else prepared[1].tolist(), "reset": adapter.reset_next})
    add({"op": "prepare", "steps": steps}, answers, "adapter_packing")
    generator = np.random.default_rng(44)
    for _ in range(20):
        values = generator.uniform(-12, 12, 18452).astype(np.float32)
        parsed = Parser().parse_outputs({k: values[np.newaxis, slice(*v)].copy() for k, v in CONTRACT["output_slices"].items()})
        wanted = {k: v.ravel().tolist() for k, v in parsed.items() if k not in ("hidden_state", "pad")}
        wanted["action"] = wanted["action"][:2]
        add({"op": "parse", "values": values.tolist()}, wanted, "adapter_parser")
    calls = 0
    failing = False
    def operation():
        nonlocal calls
        calls += 1
        if failing:
            raise RuntimeError("fixture failure")
    owner = GadgetOwner(operation, operation)
    steps, answers = [], []
    for _ in range(500):
        enable, offroad, egpu, failing = (rng.choice((False, True)) for _ in range(4))
        try:
            if enable:
                owner.enable("shadow", offroad, egpu)
            else:
                owner.disable(offroad)
            okay = True
        except RuntimeError:
            okay = False
        steps.append({"enable": enable, "offroad": offroad, "egpu": egpu, "fail": failing})
        answers.append({"ok": okay, "enabled": owner.enabled, "calls": calls})
    add({"op": "owner", "steps": steps}, answers, "offroad_ownership")
    add({"op": "descriptors"}, {"descriptors": list(build_descriptors()), "strings": list(build_strings())}, "functionfs_descriptors")
    for length, gadget in itertools.product((0, 1, 992, 1024, 16352, 16384, 393272), (False, True)):
        payload = bytes(index % 251 for index in range(length))
        total, flags = 32 + length, 0
        if gadget:
            pad = -total % 16384
        else:
            pad = int(total % 1024 == 0)
            flags = protocol.Flag.PADDED if pad else 0
        answer = protocol.pack_header(8, 42, length, flags) + payload + bytes(pad)
        add({"op": "wire", "kind": 8, "sequence": 42, "payload": list(payload), "gadget": gadget}, list(answer), "usb_wire")
    input_text = "".join(json.dumps(request) + "\n" for request in requests)
    (evidence / "oracle-input.jsonl").write_text(input_text)
    result = subprocess.run([str(binary)], input=input_text, text=True, capture_output=True, check=True)
    (evidence / "oracle-output.jsonl").write_text(result.stdout)
    (evidence / "oracle-stderr.log").write_text(result.stderr or "no stderr\n")
    actual = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(actual) == len(expected)
    maximum = 0.0
    for label, answer, observed in zip(labels, expected, actual, strict=True):
        if label == "adapter_parser":
            assert observed.keys() == answer.keys()
            for key in answer:
                a, b = np.array(answer[key]), np.array(observed[key])
                np.testing.assert_allclose(a, b, rtol=1e-6, atol=1e-6, err_msg=key)
                maximum = max(maximum, float(np.max(np.abs(a-b))))
        else:
            assert observed == answer, (label, observed, answer)
    summary = {"result": "PASS", "transition_decisions": 40000, "adapter_frames": 500, "parser_frames": 20,
               "owner_steps": 500, "cases": len(expected), "float_atol": 1e-6, "float_rtol": 1e-6, "maximum_absolute_difference": maximum,
               "discrete_and_wire": "exact", "device_or_gadget_access": False}
    (evidence / "oracle-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))


if __name__ == "__main__":
    main()

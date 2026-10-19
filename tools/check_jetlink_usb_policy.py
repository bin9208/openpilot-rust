#!/usr/bin/env python3
"""CI oracle: execute actual hash-pinned upstream policies against the Rust port."""
import ast
import hashlib
import importlib.util
import json
import logging
from pathlib import Path
import random
import subprocess
import sys
import textwrap
import tempfile
import types
import urllib.request

PIN = "d24328d849b0e5154b55f149dc18c5dfe8bd79cc"
HASHES = {
    "port.py": "c8d6dd178e2eedbfebece93a3bdc9323dc44e863836f003d24022899fad14294",
    "owner.py": "7d8a7bb1141c961d7e437be8de5887348000457c5a40909d32eb253a52ba57f9",
}


def reference(cache):
    cache.mkdir(parents=True, exist_ok=True)
    for name, expected in HASHES.items():
        data = urllib.request.urlopen(f"https://raw.githubusercontent.com/zoompilot/jetlink/{PIN}/jetlink/comma/{name}", timeout=60).read()
        assert hashlib.sha256(data).hexdigest() == expected, name
        (cache / name).write_bytes(data)
    gadget = types.SimpleNamespace(log=logging.getLogger("oracle"))
    root = types.SimpleNamespace(PORT_TIMEOUT=2)
    sys.modules["jetlink"] = types.ModuleType("jetlink")
    sys.modules["jetlink.comma"] = types.SimpleNamespace(gadget=gadget, root=root)
    spec = importlib.util.spec_from_file_location("policy_port", cache / "port.py")
    port = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(port)
    source = (cache / "owner.py").read_text()
    cls = next(node for node in ast.parse(source).body if isinstance(node, ast.ClassDef) and node.name == "Owner")
    method = next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == "steer_draw")
    scope = dict(root=root, port=port, gadget=gadget)
    exec(compile(textwrap.dedent(ast.get_source_segment(source, method)), "upstream-owner-steer_draw", "exec"), scope)
    return port, root, gadget, scope["steer_draw"]


def fixtures():
    cases = []
    # Boot/owner restart, attached/unknown restore deferral, settled failure.
    for success in (False, True):
        for start in (False, True):
            cases.append([dict(op="draw", wanted=want, role=role, success=success)
                          for want in (start, start, True, True, False, False)
                          for role in (None, "source", "sink", "none")])
    # Stable plugs exercise all timers, swap limits, failed actions and eGPU.
    for role, data in (("source", "dfp"), ("source", "ufp"), ("sink", "dfp"), ("sink", "ufp"), ("none", None)):
        for success in (False, True):
            for egpu in (False, True):
                case = [dict(now=n / 2, role=role, data=data, udc_mode="host", success=success, egpu=egpu) for n in range(80)]
                case += [dict(now=40 + n / 2, role="none", udc_mode="peripheral") for n in range(16)]
                case += [dict(now=48 + n / 2, role=role, data=data) for n in range(16)] + [dict(op="off")]
                cases.append(case)
    # Charging defaults off in this fork, but preserve the upstream opt-in policy.
    for success in (False, True):
        cases.append([dict(now=n / 2, role="sink", data="ufp", configured=True, host_attached=True,
                           charge=True, contract="explicit", hottest_c=100 if n < 12 else 70, success=success)
                      for n in range(40)] + [dict(now=21, role="source", data="ufp", configured=False)])
    rng = random.Random(491)
    for _ in range(80):
        case = []
        now = 0
        for _ in range(120):
            now += rng.choice((0.5, 1, 2, 3, 5, 10))
            role = rng.choice((None, "none", "sink", "source"))
            case.append(dict(now=now, role=role, data=rng.choice((None, "none", "dfp", "ufp")),
                             udc_mode=rng.choice(("", "none", "host", "peripheral")),
                             configured=rng.choice((False, True)), host_attached=rng.choice((False, True)),
                             egpu=rng.random() < .1, success=rng.random() > .1,
                             charge=rng.choice((False, True)), contract=rng.choice(("", "explicit")),
                             hottest_c=rng.choice((None, 75, 89, 90, 99))))
        cases.append(case)
    return cases


def main():
    binary, cache = sys.argv[1:]
    logging.disable(logging.CRITICAL)
    port, root, gadget, steer = reference(Path(cache))
    cases = fixtures()
    expected = []
    keys = "cleared deferred held settled role role_since roles roles_since plugged asks next_ask next_start charge_asked charging charge_since said_hot temp".split()
    for case in cases:
        policy = port.Port(())
        draw = types.SimpleNamespace(cut=False, draw_for=None)
        rows = []
        for step in case:
            calls = []
            def call(*args, **kwargs):
                calls.append("-".join(args))
                return step.get("success", True)
            root.run = call
            port.run_script = lambda action: call("port", action)
            port.run_udc = lambda action: call("udc", action)
            port.power_role = lambda: step.get("role")
            port.data_role = lambda: step.get("data")
            port.chestnut_attached = lambda _: step.get("egpu", False)
            port.thermal_zones = lambda: []
            port.hottest = lambda _: step.get("hottest_c")
            gadget.host_attached = lambda: step.get("host_attached", False)
            gadget.read = lambda path: step.get("udc_mode", "") if path == port.UDC_MODE else step.get("contract", "") if path == port.CONTRACT else ""
            if step.get("op") == "draw":
                steer(draw, step["wanted"])
            elif step.get("op") == "off":
                policy.off()
            else:
                policy.update(now=step["now"], configured=step.get("configured", False), charge=step.get("charge", False))
            rows.append(dict(calls=calls, cut=draw.cut, draw_for=draw.draw_for, **{key: getattr(policy, key) for key in keys}))
        expected.append(rows)
    actual = json.loads(subprocess.check_output([binary], input=json.dumps(cases).encode()))
    # JSON normalizes the actual source's role tuples to arrays.
    expected = json.loads(json.dumps(expected))
    for i, (want, got) in enumerate(zip(expected, actual, strict=True)):
        for j, (left, right) in enumerate(zip(want, got, strict=True)):
            assert left == right, json.dumps(dict(case=i, step=j, input=cases[i][j], expected=left, actual=right))
    check_boot()
    print(json.dumps(dict(upstream=PIN, cases=len(cases), steps=sum(map(len, cases)), result="exact policy equivalence")))


def check_boot():
    # Execute the real pre-installer shell path without an existing native
    # launcher. Only literal hardware paths and sudo are replaced in CI.
    source = Path("launch_openpilot.sh").read_text().split('  bash "$RUST_LAUNCH_ROOT/rust/tools/ensure_runtime.sh"', 1)[0] + "fi\n"
    for mode in (None, b"", b"0\n", b"1", b"2\n", b"3\n"):
        for first_fails in (False, True):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "AGNOS").touch()
                setting = root / "mode"
                if mode is not None: setting.write_bytes(mode)
                val, active = root / "force_val", root / "force_active"
                if first_fails: val.mkdir()
                else: val.write_text("999\n")
                active.write_text("0\n")
                script = source.replace("/AGNOS", str(root / "AGNOS")).replace("/data/params/d/JetlinkMode", str(setting)).replace("/sys/kernel/debug/pmic-votable/USB_ICL/force_val", str(val)).replace("/sys/kernel/debug/pmic-votable/USB_ICL/force_active", str(active))
                prefix = 'uname() { printf aarch64; }; sudo() { test "$1" = -n || return; shift; "$@"; };\n'
                subprocess.run(["bash", "-c", prefix + script], check=True, capture_output=True)
                wanted = mode not in (None, b"", b"0\n")
                assert active.read_text() == ("1\n" if wanted and not first_fails else "0\n")
                if not first_fails: assert val.read_text() == ("0\n" if wanted else "999\n")


if __name__ == "__main__":
    main()

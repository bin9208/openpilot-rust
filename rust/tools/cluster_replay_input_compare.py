# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==12.1.1", "numpy==2.4.2"]
# ///
# Run in the source Raylib environment with DISPLAY set to an owned Xvfb:
# python cluster_replay_input_compare.py ROOT OUTPUT [NATIVE_BINARY]
"""Compare real replay-window mouse, key, tuning and seek outcomes."""
from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time
from typing import TypedDict

import numpy as np
from PIL import Image

from cluster_replay_x11 import XInput


class Control(TypedDict):
    seek_s: float | None
    corner_lateral_offset_m: float
    control_active: bool


class Response(TypedDict):
    name: str
    input: Control
    mouse_down: bool
    height_m: float
    pitch_deg: float
    path: str


@dataclass(frozen=True, slots=True)
class Step:
    name: str
    point: tuple[float, float] = (10.0, 10.0)
    down: bool = False
    keys: tuple[str, ...] = ()
    duration_s: float = 100.0
    tuning_visible: bool = True


def steps() -> list[Step]:
    result = [Step("idle")]
    for name, x, y in (("left", 482, 424), ("middle", 860, 424), ("right", 1238, 424), ("top", 860, 416), ("bottom", 860, 432), ("above", 860, 414), ("below", 860, 434), ("before", 480, 424), ("after", 1240, 424)):
        result.append(Step(f"seek-{name}", (x, y), True))
    result.extend([Step("released", (860, 424)), Step("zero-duration", (860, 424), True, ("bracketright",), 0.0), Step("negative-duration", (860, 424), True, (), -1.0), Step("disabled-key", keys=("bracketright",), tuning_visible=False), Step("disabled-seek", (860, 424), True, (), tuning_visible=False)])
    for key in ("bracketleft", "bracketright", "semicolon", "apostrophe", "0"):
        result.append(Step(f"key-{key}", keys=(key,)))
    result.append(Step("ordered-keys-reset", keys=("bracketleft", "bracketright", "semicolon", "apostrophe", "0")))
    for index in range(22):
        result.append(Step(f"height-upper-clamp-{index}", keys=("bracketright",)))
    for index in range(22):
        result.append(Step(f"pitch-upper-clamp-{index}", keys=("apostrophe",)))
    for name, x in (("min", 482), ("middle", 860), ("max", 1238)):
        result.extend([Step(f"height-bar-{name}", (x, 368), True), Step(f"pitch-bar-{name}", (x, 334), True)])
    result.extend([Step("reset", keys=("0",)), Step("reset-unchanged", keys=("0",)), Step("idle-final")])
    expanded = []
    for step in result:
        expanded.append(step)
        if step.keys:
            expanded.append(Step(f"{step.name}-keyup", step.point, step.down, duration_s=step.duration_s, tuning_visible=step.tuning_visible))
    return expanded


class Protocol:
    def __init__(self, process: subprocess.Popen[bytes], log: Path) -> None:
        assert process.stdout is not None
        self.process = process
        self.output = process.stdout
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.output, selectors.EVENT_READ)
        self.pending = b""
        self.log = log.open("wb")

    def read(self) -> Response:
        deadline = time.monotonic() + 10.0
        while time.monotonic() < deadline:
            while b"\n" in self.pending:
                line, self.pending = self.pending.split(b"\n", 1)
                if line.startswith(b"{"):
                    return json.loads(line)
            assert self.selector.select(max(0.0, deadline - time.monotonic())), "window protocol timeout"
            data = os.read(self.output.fileno(), 65536)
            assert data, ("window protocol EOF", self.process.poll())
            self.log.write(data)
            self.log.flush()
            self.pending += data
        raise TimeoutError("window protocol timeout")

    def close(self) -> None:
        self.selector.close()
        self.log.close()


def run(command: list[str], output: Path, width: int, title: str) -> list[Response]:
    output.mkdir(parents=True, exist_ok=True)
    (output / "command.json").write_text(json.dumps(command))
    records = []
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as process:
        protocol, x = Protocol(process, output / "driver.log"), XInput()
        try:
            assert protocol.read().get("ready") is True
            window = x.window(title)
            for step in steps():
                if step.name.endswith("-keyup"):
                    x.release_keys(previous.keys)
                x.input(window, width / 1920.0, step.point, step.down, step.keys)
                assert process.stdin is not None
                process.stdin.write((json.dumps(asdict(step)) + "\n").encode())
                process.stdin.flush()
                row = protocol.read()
                assert row["name"] == step.name
                records.append(row)
                previous = step
            process.stdin.close()
            assert process.wait(timeout=10.0) == 0
        finally:
            x.close()
            protocol.close()
            if process.poll() is None:
                process.kill()
                process.wait(timeout=10.0)
    (output / "result.json").write_text(json.dumps(records, indent=2))
    return records


def main() -> None:
    root, output = (Path(value).resolve() for value in sys.argv[1:3])
    binary = Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else None
    output.mkdir(parents=True, exist_ok=True)
    failures = []
    for width, height in ((1920, 480), (1280, 320)):
        title = "cluster replay input fixture"
        input_path = output / f"input-{width}.json"
        input_path.write_text(json.dumps({"options": {"width": width, "height": height, "title": title, "target_fps": 0, "theme_mode": "light", "screen_mode": 6, "panel_layout": 0, "language": "en", "is_metric": True}}))
        source_dir = output / f"source-{width}"
        expected = run([sys.executable, str(root / "rust/tools/cluster_replay_input_source.py"), str(root), str(input_path), str(source_dir)], source_dir, width, title)
        for row in expected:
            with Image.open(row["path"]) as image:
                assert image.size == (width, height)
                assert image.convert("RGB").getcolors(20) is None
        if binary is None:
            continue
        native_dir = output / f"native-{width}"
        actual = run([str(binary), str(root), str(input_path), str(native_dir)], native_dir, width, title)
        for source_row, native_row in zip(expected, actual, strict=True):
            fields = ("input", "mouse_down", "height_m", "pitch_deg")
            errors = [field for field in fields if source_row[field] != native_row[field]]
            with Image.open(source_row["path"]) as a, Image.open(native_row["path"]) as b:
                assert a.size == b.size == (width, height)
                changed = int(np.any(np.asarray(a.convert("RGBA")) != np.asarray(b.convert("RGBA")), axis=2).sum())
                assert a.convert("RGB").getcolors(20) is None
            if errors or changed:
                failures.append({"name": source_row["name"], "width": width, "fields": errors, "pixels": changed, "source": source_row, "native": native_row})
    receipt = {"steps_per_size": len(steps()), "sizes": 2, "failures": failures, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest() if binary else None, "source_sha256": hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py").read_bytes()).hexdigest()}
    (output / "comparison.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps({"steps_per_size": len(steps()), "failures": len(failures)}))
    assert not failures


if __name__ == "__main__":
    main()

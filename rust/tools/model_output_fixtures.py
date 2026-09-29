from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

import numpy as np
from numpy.typing import NDArray


@dataclass(frozen=True, slots=True)
class OutputFixture:
    slices: dict[str, tuple[int, int]]
    frames: tuple[NDArray[np.float32], ...]
    kind: Literal["driving", "driver"]
    action_inputs: dict[str, float] | None = None


def layout(kind: Literal["driving", "driver"], mixture: bool = False) -> dict[str, tuple[int, int]]:
    widths = {"meta": 55, "desire_pred": 32, "pose": 12, "wide_from_device_euler": 6, "road_transform": 12,
                  "lane_lines": 528, "lane_lines_prob": 8, "road_edges": 264, "lead": 102 if mixture else 144,
                  "lead_prob": 3, "hidden_state": 512, "plan": 990, "desire_state": 8}
    if kind == "driver":
        widths = {}
        for suffix in ("lhd", "rhd"):
            widths[f"face_descs_{suffix}"] = 12
            for key in ("face_prob", "left_eye_prob", "right_eye_prob", "left_blink_prob", "right_blink_prob",
                        "sunglasses_prob", "using_phone_prob", "sleep_prob"):
                widths[f"{key}_{suffix}"] = 1
        widths["wheel_on_right"] = 1
        widths["features"] = 512
    offset = 0
    slices = {}
    for name, width in widths.items():
        slices[name] = (offset, offset + width)
        offset += width
    return slices


def synthetic(kind: Literal["driving", "driver"], mixture: bool = False) -> OutputFixture:
    slices = layout(kind, mixture)
    random = np.random.default_rng(20260930)
    frames = []
    for frame in range(240 if kind == "driving" else 16):
        values = random.normal(0, 2, max(end for _, end in slices.values())).astype(np.float32)
        if kind == "driving":
            start, _ = slices["plan"]
            plan = values[start:start + 495].reshape(33, 15)
            plan[:, 0] = np.linspace(0, 300, 33)
            plan[:, 3] = np.linspace(8, 20, 33)
            if frame % 8 == 0:
                plan[:, 0] = 0
            if frame % 8 == 1:
                plan[:, 0] = np.linspace(40, -40, 33)
            if frame == 2:
                plan[12, 0] = np.nan
            if frame == 3:
                plan[12, 0] = np.inf
            meta_start, meta_end = slices["meta"]
            values[meta_start:meta_end] = -8
            values[meta_start + 4:meta_start + 31:6] = 2
            values[meta_start + 6:meta_start + 31:6] = -1 if frame % 12 < 6 else -8
            if frame >= 120:
                values[meta_start:meta_end] = 3
            if frame >= 200:
                values[meta_start:meta_end] = 100
            if mixture and frame % 2 == 0:
                lead_start, _ = slices["lead"]
                values[lead_start + 48:lead_start + 51] = 0
                values[lead_start + 99:lead_start + 102] = 0
            if mixture and frame < 7:
                lead_start, _ = slices["lead"]
                values[lead_start + 48:lead_start + 51] = [1e-8, 2e-8, 3e-8, 4e-8, 5e-8, 6e-8, 1e-7][frame]
                values[lead_start + 99:lead_start + 102] = 0
        else:
            for name, (start, end) in slices.items():
                if name != "features" and frame in (0, 1, 2):
                    values[start:end] = (-100, 100, np.nan)[frame]
        frames.append(values)
    return OutputFixture(slices, tuple(frames), kind)


def interpolation_edges() -> Iterator[tuple[str, OutputFixture]]:
    slices = layout("driving")
    cases = [("positive_left", 0.2, np.inf, 0.25), ("negative_left", 0.2, -np.inf, 0.25),
             ("equal_positive", 0.2, np.inf, np.inf), ("equal_negative", 0.2, -np.inf, -np.inf),
             ("exact_nan", 0.244140625, np.nan, 0.0), ("exact_positive", 0.244140625, np.inf, 0.0),
             ("exact_negative", 0.244140625, -np.inf, 0.0)]
    for column in (3, 11):
        for name, time, left, right in cases:
            values = np.zeros(max(end for _, end in slices.values()), dtype=np.float32)
            start, _ = slices["plan"]
            plan = values[start:start + 495].reshape(33, 15)
            plan[:, 11] = 0.25
            plan[4, column] = left
            plan[5, column] = right
            inputs = {"lat_action_t": time, "long_action_t": time, "v_ego": 10.0,
                      "lat_smooth_seconds": 0.15, "v_ego_stopping": 0.05}
            yield f"interpolation-{column}-{name}", OutputFixture(slices, (values,), "driving", inputs)
    values = np.zeros(max(end for _, end in slices.values()), dtype=np.float32)
    start, _ = slices["plan"]
    values[start + 11 * 15 + 3] = -np.inf
    inputs = {"lat_action_t": 0.2, "long_action_t": 0.2, "v_ego": 10.0,
              "lat_smooth_seconds": 0.15, "v_ego_stopping": 0.05}
    yield "interpolation-stop-preview", OutputFixture(slices, (values,), "driving", inputs)


def captured(pipeline: Path, output: Path, kind: Literal["driving", "driver"]) -> Iterator[OutputFixture]:
    import json
    descriptor = json.loads(pipeline.read_text())
    slices = descriptor["metadata"]["output_slices"]
    paths = sorted(output.glob("model-*.bin"), key=lambda path: int(path.stem.split("-")[-1]))
    if not paths:
        raise FileNotFoundError(f"no captured model outputs in {output}")
    yield OutputFixture(slices, tuple(np.fromfile(path, dtype=np.float32) for path in paths), kind)

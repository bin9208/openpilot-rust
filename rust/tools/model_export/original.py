from __future__ import annotations

import math
import pickle
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import TypedDict

from tinygrad import Tensor, TinyJit

from openpilot.common.file_chunker import open_file_chunked
from openpilot.selfdrive.modeld.helpers import load_oob

from .pipeline import Pipeline, Stage, compose, restore_inputs
from .schema import Bindings, ExportError


class OriginalMetadata(TypedDict):
    model_checkpoint: str
    input_shapes: dict[str, tuple[int, ...]]
    output_shapes: dict[str, tuple[int, ...]]
    output_slices: dict[str, slice]


@dataclass(frozen=True, slots=True)
class Metadata:
    model_checkpoint: str
    input_shapes: Mapping[str, tuple[int, ...]]
    output_shapes: Mapping[str, tuple[int, ...]]
    output_slices: Mapping[str, tuple[int, int]]

    @classmethod
    def from_original(cls, raw: OriginalMetadata) -> Metadata:
        if not isinstance(raw["model_checkpoint"], str) or not 0 < len(raw["model_checkpoint"]) <= 4096 or len(raw["output_shapes"]) != 1:
            raise ExportError("model requires a checkpoint and one flat output")
        for shapes in (raw["input_shapes"], raw["output_shapes"]):
            if not shapes or len(shapes) > 1024:
                raise ExportError("invalid model shape count")
            for name, shape in shapes.items():
                if not name or not shape or len(shape) > 16 or any(type(size) is not int or size <= 0 for size in shape):
                    raise ExportError("static positive model shapes required")
                if math.prod(shape) > 1024**3:
                    raise ExportError("model tensor too large")
        size = math.prod(next(iter(raw["output_shapes"].values())))
        slices = {}
        for name, value in raw["output_slices"].items():
            if not name or not isinstance(value, slice) or value.step not in (None, 1):
                raise ExportError("contiguous named model output slices required")
            if any(endpoint is not None and (type(endpoint) is not int or not -size <= endpoint <= size)
                   for endpoint in (value.start, value.stop)):
                raise ExportError("model output slice outside tensor")
            start, end, _ = value.indices(size)
            if start >= end:
                raise ExportError("empty model output slice")
            slices[name] = (start, end)
        if not slices or len(slices) > 1024:
            raise ExportError("invalid model output slice count")
        return cls(raw["model_checkpoint"], raw["input_shapes"], raw["output_shapes"], slices)


@dataclass(frozen=True, slots=True)
class OriginalPipeline:
    metadata: Metadata
    stages: tuple[Stage, ...]
    bindings: Bindings
    pipeline: Pipeline


def driving(path: Path, resolution: tuple[int, int]) -> OriginalPipeline:
    with open_file_chunked(str(path)) as source:
        artifact = load_oob(source)
    warp, policy = artifact[resolution], artifact["run_policy"]
    if not isinstance(warp, TinyJit) or not isinstance(policy, TinyJit) or warp.captured is None or policy.captured is None:
        raise ExportError("original driving artifact requires captured warp and policy")
    if not isinstance(warp.captured.ret, Tensor) or len(policy.captured.ret) != 1 or not isinstance(policy.captured.ret[0], Tensor):
        raise ExportError("original driving output contract")
    warp_inputs = restore_inputs(warp)
    policy_inputs = restore_inputs(policy, {"warped":warp.captured.ret})
    inputs = dict(warp_inputs) | {name: tensor for name, tensor in policy_inputs.items() if name != "warped"}
    if len(inputs) != len(warp_inputs) + len(policy_inputs) - 1:
        raise ExportError("ambiguous original driving inputs")
    outputs = {"model":policy.captured.ret[0], "warped":warp.captured.ret}
    outputs.update({name:policy_inputs[name] for name in ("img_q", "big_img_q", "feat_q", "desire_q")})
    bindings = Bindings(inputs, outputs)
    stages = (Stage("prepare", warp, warp_inputs), Stage("policy", policy, policy_inputs))
    return OriginalPipeline(Metadata.from_original(artifact["metadata"]), stages, bindings, compose(stages, bindings))


@dataclass(frozen=True, slots=True)
class DriverArtifacts:
    model: Path
    warp: Path
    metadata: Path


def driver(paths: DriverArtifacts) -> OriginalPipeline:
    with open_file_chunked(str(paths.model)) as source:
        model = pickle.load(source)
    with paths.warp.open("rb") as source:
        warp = pickle.load(source)
    with paths.metadata.open("rb") as source:
        metadata = Metadata.from_original(pickle.load(source))
    if not isinstance(warp, TinyJit) or not isinstance(model, TinyJit) or warp.captured is None or model.captured is None:
        raise ExportError("original driver artifact requires captured warp and model")
    if not isinstance(warp.captured.ret, Tensor) or not isinstance(model.captured.ret, Tensor):
        raise ExportError("original driver output contract")
    warp_inputs = restore_inputs(warp)
    model_inputs = restore_inputs(model, {"input_img":warp.captured.ret})
    if set(warp_inputs) != {"0", "1"} or set(model_inputs) != {"input_img", "calib"}:
        raise ExportError("original driver input contract")
    bindings = Bindings({"frame":warp_inputs["0"], "transform":warp_inputs["1"], "calib":model_inputs["calib"]},
                        {"model":model.captured.ret, "warped":warp.captured.ret})
    stages = (Stage("prepare", warp, warp_inputs), Stage("model", model, model_inputs))
    return OriginalPipeline(metadata, stages, bindings, compose(stages, bindings))

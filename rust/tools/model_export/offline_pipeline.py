from __future__ import annotations

from collections.abc import Callable, Generator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, assert_never

from tinygrad import Context, Tensor, dtypes
from tinygrad.engine.realize import capturing
from tinygrad.uop.ops import Ops, UOp

from openpilot.common.transformations.model import DM_INPUT_SIZE, MEDMODEL_INPUT_SIZE
from openpilot.selfdrive.modeld.compile_dm_warp import make_warp_dm
from openpilot.selfdrive.modeld.compile_modeld import NV12Frame, make_input_queues, make_run_policy, make_warp
from openpilot.selfdrive.modeld.constants import ModelConstants
from openpilot.selfdrive.modeld.get_model_metadata import make_metadata_dict
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info

from .offline_capability import static_movement_capability
from .offline_model import InitializedBuffer, OfflineModel
from .offline_devices import host_build
from .offline_onnx import OfflineOnnx, load_runner
from .offline_schedule import export_pipeline as export_pipeline
from .original import Metadata
from .schema import Bindings, ExportError


@dataclass(frozen=True, slots=True)
class OfflinePipelineSpec:
    kind: Literal["driving", "driver"]
    model: Path
    sha256: str
    camera: tuple[int, int]


@dataclass(frozen=True, slots=True)
class OfflineStage:
    name: str
    linear: UOp


@dataclass(frozen=True, slots=True)
class OfflinePipeline:
    spec: OfflinePipelineSpec
    model: OfflineModel
    metadata: Metadata
    stages: tuple[OfflineStage, ...]


class ScheduleCollector:
    def __init__(self) -> None:
        self.linears: list[UOp] = []

    def add_linear(self, linear: UOp, variables: dict[str, int]) -> None:
        if variables:
            raise ExportError("offline pipeline requires static dimensions")
        self.linears.append(linear)


@contextmanager
def collect_schedule(collector: ScheduleCollector) -> Generator[None, None, None]:
    if capturing:
        raise ExportError("offline pipeline capture cannot be nested")
    capturing.append(collector)
    try:
        with static_movement_capability(), Context(DEV="QCOM", CAPTURING=1, NO_MEMORY_PLANNER=1, IMAGE=1,
                                                   FLOAT16=1, NOLOCALS=1, BEAM=0, OPENPILOT_HACKS=1):
            yield
    finally:
        capturing.remove(collector)


def capture_stage(name: str, function: Callable[[], tuple[Tensor, ...]]) -> tuple[OfflineStage, tuple[Tensor, ...]]:
    collector = ScheduleCollector()
    with collect_schedule(collector):
        outputs = function()
        Tensor.realize(*outputs)
    return OfflineStage(name, UOp(Ops.LINEAR, src=tuple(call for linear in collector.linears for call in linear.src))), outputs


def driver_pipeline(loaded: OfflineOnnx, spec: OfflinePipelineSpec, nv12: NV12Frame) -> OfflinePipeline:
    with Context(DEV="CPU:LLVM"):
        metadata = Metadata.from_original(make_metadata_dict(spec.model))
    frame = Tensor.empty(nv12.size, dtype=dtypes.uint8, device="QCOM")
    transform = Tensor.empty(3, 3, dtype=dtypes.float32, device="NPY")
    calib = Tensor.empty(*metadata.input_shapes["calib"], dtype=dtypes.float32, device="NPY")
    warp = make_warp_dm(nv12, *DM_INPUT_SIZE)
    prepare, (warped,) = capture_stage("prepare", lambda: (warp(frame, transform),))
    model_stage, (output,) = capture_stage("model", lambda: (next(iter(loaded.runner({"input_img": warped,
                                                                                     "calib": calib.to("QCOM")}).values()))
                                                              .cast(dtypes.float32),))
    bindings = Bindings({"frame": frame, "transform": transform, "calib": calib}, {"model": output, "warped": warped})
    return OfflinePipeline(spec, OfflineModel(bindings, loaded.initialized), metadata, (prepare, model_stage))


def driving_pipeline(loaded: OfflineOnnx, spec: OfflinePipelineSpec, nv12: NV12Frame) -> OfflinePipeline:
    with Context(DEV="CPU:LLVM"):
        raw = make_metadata_dict(spec.model)
    frame_skip = ModelConstants.MODEL_RUN_FREQ // ModelConstants.MODEL_CONTEXT_FREQ
    with Context(DEV="CPU:LLVM"):
        original_queues, _ = make_input_queues(raw["input_shapes"], frame_skip, "CPU")
    queues = {}
    initialized = list(loaded.initialized)
    for name, tensor in original_queues.items():
        if name in {"tfm", "big_tfm", "packed_npy_inputs"}:
            queues[name] = tensor
        else:
            queues[name] = Tensor.empty(*tensor.shape, dtype=tensor.dtype, device="QCOM")
            initialized.append(InitializedBuffer(queues[name].uop.base, tensor.data().tobytes()))
    frame = Tensor.empty(nv12.size, dtype=dtypes.uint8, device="QCOM")
    big_frame = Tensor.empty(nv12.size, dtype=dtypes.uint8, device="QCOM")
    warp = make_warp(nv12, *MEDMODEL_INPUT_SIZE, frame_skip)
    prepare, (warped,) = capture_stage("prepare", lambda: (warp(queues["tfm"], queues["big_tfm"], frame, big_frame),))
    policy = make_run_policy(loaded.runner, raw, frame_skip)
    policy_stage, (output,) = capture_stage("policy", lambda: policy(warped, queues["img_q"], queues["big_img_q"],
                                                                    queues["feat_q"], queues["desire_q"], queues["packed_npy_inputs"]))
    bindings = Bindings({"tfm": queues["tfm"], "big_tfm": queues["big_tfm"], "frame": frame, "big_frame": big_frame,
                        **{name: queues[name] for name in ("img_q", "big_img_q", "feat_q", "desire_q", "packed_npy_inputs")}},
                       {"model": output, "warped": warped, **{name: queues[name] for name in ("img_q", "big_img_q", "feat_q", "desire_q")}})
    return OfflinePipeline(spec, OfflineModel(bindings, tuple(initialized)), Metadata.from_original(raw), (prepare, policy_stage))


@host_build
def build_pipeline(spec: OfflinePipelineSpec) -> OfflinePipeline:
    if spec.camera not in {(1928, 1208), (1344, 760)}:
        raise ExportError("unsupported offline pipeline camera")
    loaded = load_runner(spec.model, spec.sha256)
    nv12 = NV12Frame(*spec.camera, *get_nv12_info(*spec.camera))
    match spec.kind:
        case "driver":
            return driver_pipeline(loaded, spec, nv12)
        case "driving":
            return driving_pipeline(loaded, spec, nv12)
        case unreachable:
            assert_never(unreachable)

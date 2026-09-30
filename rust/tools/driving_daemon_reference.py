"""Original source statements and compiled model state for driving-daemon QA."""
from __future__ import annotations

import ast
import hashlib
import time
from pathlib import Path
from types import SimpleNamespace

import numpy as np

from check_desire_reference import Commands, original_class
from check_model_inputs import SubMaster, source_nodes
from check_pipeline_reference import execute, write_tensor
from model_export.original import driving
from model_output_reference import new_message, original_functions
from openpilot.cereal import log
from openpilot.common.filter_simple import FirstOrderFilter
from openpilot.common.transformations.camera import DEVICE_CAMERAS
from openpilot.common.transformations.model import get_warp_matrix
from openpilot.selfdrive.modeld import fill_model_msg
from openpilot.selfdrive.modeld.compile_modeld import get_policy_npy_shapes
from openpilot.selfdrive.modeld.parse_model_outputs import Parser
from openpilot.selfdrive.modeld.jetlink.runtime import Runtime as JetlinkRuntime
from openpilot.selfdrive.modeld.jetlink.transition import ControlState

TOPICS = ("deviceState", "carState", "roadCameraState", "liveCalibration", "driverMonitoringState", "carControl", "liveDelay", "carrotMan", "radarState")


def source_blocks():
    path = Path(__file__).resolve().parents[2] / "openpilot/selfdrive/modeld/modeld.py"
    tree = ast.parse(path.read_text())
    main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
    loop = next(node for node in main.body if isinstance(node, ast.While) and any(ast.unparse(child).startswith("desire = DH.desire") for child in node.body))
    start = next(i for i, node in enumerate(loop.body) if ast.unparse(node).startswith("desire = DH.desire"))
    end = next(i for i, node in enumerate(loop.body) if isinstance(node, ast.AnnAssign) and ast.unparse(node.target) == "inputs")
    result = next(node for node in loop.body if isinstance(node, ast.If) and ast.unparse(node.test) == "model_output is not None")
    final = next(i for i, node in enumerate(result.body) if ast.unparse(node).startswith("pm.send(")) - 1
    dynamic = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "get_lat_smooth_seconds_dynamic")
    blocks = ([dynamic], loop.body[start:end + 1], result.body[:final + 1])
    return tuple(compile(ast.Module(body=nodes, type_ignores=[]), str(path), "exec") for nodes in blocks)


class Oracle:
    """Mutable original-source loop and actual compiled-model state for one camera session."""
    def __init__(self, models: Path, resolution: tuple[int, int], mode: str):
        self.original = driving(models / "driving_tinygrad.pkl", resolution)
        self.jetlink = JetlinkRuntime(SimpleNamespace(get_bool=lambda _: False, get=lambda _: None, put_bool=lambda *_: None), resolution, None)
        self.dynamic, self.before, self.after = source_blocks()
        _, _, self.pulse, _ = source_nodes()
        shapes, sizes = get_policy_npy_shapes(self.original.metadata.input_shapes)
        self.packed = np.zeros(sum(sizes), dtype=np.float32)
        fields = {key: value.reshape(shape) for (key, shape), value in zip(shapes.items(), np.split(self.packed, np.cumsum(sizes[:-1])), strict=True)}
        self.model = SimpleNamespace(prev_desire=np.zeros(8, dtype=np.float32), npy=fields, vision_input_names=("img", "big_img"))
        helper = original_class()()
        helper.bluetooth_commands = Commands(None)
        action, _ = original_functions()
        self.scope = dict(action.__dict__, DH=helper, model=self.model, publish_state=fill_model_msg.PublishState(),
                          prev_action=log.ModelDataV2.Action.new_message(), lat_delay_dynamic=0., long_delay=.3,
                          lat_smooth_seconds=0., custom_lat_delay=0., vEgoStopping=5 * .01, camera_yaw_trim_deg=35 * .01,
                          frame_dropped_filter=FirstOrderFilter(0., 10., .05), last_vipc_frame_id=0, run_count=0,
                          model_transform_main=np.zeros((3, 3), dtype=np.float32), model_transform_extra=np.zeros((3, 3), dtype=np.float32),
                          live_calib_seen=False, DEVICE_CAMERAS=DEVICE_CAMERAS, get_warp_matrix=get_warp_matrix,
                          main_wide_camera=mode == "wide", use_extra_client=mode == "dual", buf_main=None, buf_extra=None,
                          cloudlog=SimpleNamespace(error=lambda message: None), messaging=SimpleNamespace(new_message=new_message),
                          time=time, mt1=0., SIMULATION=False, jetlink=self.jetlink, fill_model_msg=fill_model_msg.fill_model_msg,
                          fill_driving_model_data=fill_model_msg.fill_driving_model_data, fill_pose_msg=fill_model_msg.fill_pose_msg)
        exec(self.dynamic, self.scope)
        self.sm = SubMaster({name: getattr(new_message(name), name) for name in TOPICS})
        self.sm.seen = dict.fromkeys(TOPICS, False)
        self.sm.updated = dict.fromkeys(TOPICS, False)

    def prepare(self, frame_id: int, images: tuple[np.ndarray, np.ndarray]) -> None:
        metadata = SimpleNamespace(frame_id=frame_id, timestamp_eof=frame_id * 50000000 + 1000)
        self.scope.update(sm=self.sm, meta_main=metadata, meta_extra=metadata)
        exec(self.before, self.scope)
        self.jetlink.begin(0, ControlState(False, False, False, False), {}, {}, {}, frame_id, self.scope["prepare_only"])
        exec(self.pulse, dict(self.scope, self=self.model))
        for name, values in zip(("frame", "big_frame"), images, strict=True):
            write_tensor(self.original.bindings.inputs[name], values)
        for name, source in (("tfm", "model_transform_main"), ("big_tfm", "model_transform_extra")):
            write_tensor(self.original.bindings.inputs[name], self.scope[source])
        write_tensor(self.original.bindings.inputs["packed_npy_inputs"], self.packed)
        self.scope["packed_sha256"] = hashlib.sha256(self.packed.tobytes()).hexdigest()
        execute(self.original.stages[0])
        self.scope["last_vipc_frame_id"] = frame_id
        if self.scope["prepare_only"]:
            return
        execute(self.original.stages[1])
        output = self.original.bindings.outputs["model"].numpy().reshape(-1).copy()
        assert np.all(np.isfinite(output))
        values = {name: output[None, start:end].copy() for name, (start, end) in self.original.metadata.output_slices.items()}
        self.scope["model_output"] = Parser().parse_outputs(values)
        self.scope["model_output"]["raw_pred"] = output
        start, end = self.original.metadata.output_slices["hidden_state"]
        self.model.npy["prev_feat"][:] = output[start:end]

    def publications(self, execution_time: float):
        self.scope["model_execution_time"] = execution_time
        exec(self.after, self.scope)
        return [self.scope[name] for name in ("modelv2_send", "drivingdata_send", "posenet_send")]

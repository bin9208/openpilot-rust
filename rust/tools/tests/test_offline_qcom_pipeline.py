from __future__ import annotations

from pathlib import Path

import pytest

from model_export import ExportError
from model_export.offline_pipeline import OfflinePipelineSpec, build_pipeline


def test_exact_driver_pipeline_preserves_source_warp_when_camera_is_supported() -> None:
    # Given the exact source driver model and original TICI camera dimensions.
    source = Path(__file__).resolve().parents[3] / "openpilot/selfdrive/modeld/models/dmonitoring_model.onnx"
    spec = OfflinePipelineSpec("driver", source,
                               "dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04", (1928, 1208))

    # When the original warp and model graph are captured without numeric input execution.
    pipeline = build_pipeline(spec)

    # Then source external bindings and independent stage boundaries survive.
    assert tuple(stage.name for stage in pipeline.stages) == ("prepare", "model")
    assert set(pipeline.model.bindings.inputs) == {"frame", "transform", "calib"}
    assert set(pipeline.model.bindings.outputs) == {"model", "warped"}
    assert pipeline.model.bindings.outputs["warped"].shape == (1, 1440 * 960)
    assert pipeline.model.bindings.outputs["model"].shape == (1, 553)
    assert all(stage.linear.src for stage in pipeline.stages)


def test_offline_pipeline_rejects_camera_when_source_has_no_contract() -> None:
    # Given a camera outside both original build configurations.
    spec = OfflinePipelineSpec("driver", Path("unread.onnx"), "0" * 64, (100, 100))

    # When an offline pipeline is requested.
    with pytest.raises(ExportError, match="camera"):
        build_pipeline(spec)

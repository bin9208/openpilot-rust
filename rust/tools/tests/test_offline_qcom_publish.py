from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest
from tinygrad import Tensor

from model_export import offline_publish
from model_export.offline_model import OfflineModel
from model_export.offline_pipeline import OfflinePipeline, OfflinePipelineSpec
from model_export.original import Metadata
from model_export.schema import Bindings, ExportError


@pytest.fixture
def prepared(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    source = tmp_path / "source.onnx"
    source.write_bytes(b"owned publication fixture, not an inference model")
    spec = OfflinePipelineSpec("driver", source, hashlib.sha256(source.read_bytes()).hexdigest(), (1928, 1208))
    tensor = Tensor.empty(2, device="QCOM")
    pipeline = OfflinePipeline(spec, OfflineModel(Bindings({"input": tensor}, {"output": tensor}), ()),
                               Metadata("owned-publication", {"input": (2,)}, {"output": (2,)}, {"all": (0, 2)}), ())
    monkeypatch.setattr(offline_publish, "build_pipeline", lambda _: pipeline)

    def emit(_pipeline, _compiler, destination: Path) -> None:
        destination.mkdir(parents=True)
        (destination / "graph.json").write_bytes(b'{"publication_fixture":true}\n')
        (destination / "weights.bin").write_bytes(b"owned constant bytes")

    monkeypatch.setattr(offline_publish, "export_pipeline", emit)
    return spec, tmp_path / "catalog"


def test_offline_publication_preserves_source_and_native_catalog_contract(prepared) -> None:
    spec, root = prepared
    offline_publish.publish(root, [spec], None)
    pointer = json.loads((root / "current.json").read_text())
    index = root / pointer["generation"] / "index.json"
    assert hashlib.sha256(index.read_bytes()).hexdigest() == pointer["generation"]
    entry, = json.loads(index.read_text())["bundles"]
    descriptor = index.parent / entry["directory"] / "pipeline.json"
    assert hashlib.sha256(descriptor.read_bytes()).hexdigest() == entry["pipeline_sha256"]
    data = json.loads(descriptor.read_text())
    assert data["sources"] == {"onnx": spec.sha256}
    assert data["camera"] == [1928, 1208]
    assert data["kind"] == "driver"
    assert data["nv12"]["bytes"] > 1928 * 1208
    assert data["inputs"] == [{"name": "input", "shape": [2], "dtype": "float32"}]
    before = (root / "current.json").read_bytes()
    offline_publish.publish(root, [spec], None)
    assert (root / "current.json").read_bytes() == before


def test_compiler_failure_does_not_replace_existing_catalog(prepared, monkeypatch: pytest.MonkeyPatch) -> None:
    spec, root = prepared
    offline_publish.publish(root, [spec], None)
    before = (root / "current.json").read_bytes()

    def failure(*_arguments):
        raise ExportError("owned compilation failure")

    monkeypatch.setattr(offline_publish, "export_pipeline", failure)
    with pytest.raises(ExportError, match="compilation failure"):
        offline_publish.publish(root, [spec], None)
    assert (root / "current.json").read_bytes() == before
    assert not list(root.glob(".build-*"))


def test_existing_generation_corruption_cannot_be_reused(prepared) -> None:
    spec, root = prepared
    offline_publish.publish(root, [spec], None)
    before = (root / "current.json").read_bytes()
    generation = root / json.loads(before)["generation"]
    weights, = generation.rglob("weights.bin")
    weights.write_bytes(b"corrupted")
    with pytest.raises(ExportError, match="immutable"):
        offline_publish.publish(root, [spec], None)
    assert (root / "current.json").read_bytes() == before
    assert weights.read_bytes() == b"corrupted"


def test_duplicate_camera_selection_is_rejected_before_export(prepared) -> None:
    spec, root = prepared
    with pytest.raises(ExportError, match="duplicate"):
        offline_publish.publish(root, [spec, spec], None)
    assert not root.exists()
